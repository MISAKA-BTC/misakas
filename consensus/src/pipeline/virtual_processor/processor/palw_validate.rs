//! **`VirtualStateProcessor`: palw validate methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

mod panel;
mod execution;
mod reporting;
mod court;
mod model_market;
mod class_registration;
mod bond_readiness;
mod kernel_material;
mod improvement;
mod structural;

impl VirtualStateProcessor {
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
                | Obj::MaterialDisclosed { .. } => self.palw_validate_panel_object_v1(state, state_params, point, panel_params, object)?,
                Obj::ExecWorkRootOpenedV2 { .. }
                | Obj::ExecOriginRecordedV1 { .. }
                | Obj::FreePromptCommitted { .. }
                | Obj::GenTensorCommitted { .. }
                | Obj::DerivedArtifactV1 { .. } => self.palw_validate_execution_object_v1(state, state_params, point, object)?,
                Obj::ReporterCommitted { .. }
                | Obj::ReporterRevealed { .. }
                | Obj::ObjectiveOffence { .. } => self.palw_validate_reporting_object_v1(state, state_params, point, object)?,
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
                | Obj::AdapterClassListed { .. } => self.palw_validate_class_registration_object_v1(state, state_params, point, object)?,
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
                | Obj::KernelConstraintReceiptV1 { .. } => self.palw_validate_kernel_material_object_v1(state, state_params, point, object)?,
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
                Obj::SignedRegistrationV1 { .. }
                | Obj::ReservedPipelineDa84(..)
                | Obj::ReservedPipelineDa85(..) => self.palw_validate_structural_object_v1(object)?,
            }
        }
        Ok(())
    }
}
