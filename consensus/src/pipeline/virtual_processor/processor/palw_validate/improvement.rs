//! Object authentication and admission for the existing improvement family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_improvement_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            // RFC-0004 (tags 70–82): the acceptance walk drops every improvement object by name
            // before this gate, below the fence and — until each object's admission lands —
            // above it; refused here too, as the fold refuses it, so no door takes one early.
            // RFC-0004 (spec 17 §17.4.2): the owner's policy object, signed by the spec 15 owner's key.
            Obj::ModelLineImprovementPolicySet { payload, signature } => {
                if !self.palw_improvement_at(point.daa_score) {
                    return Err(format!(
                        "{} is refused: palw_improvement_v1 is not in force at this block (RFC-0004)",
                        "a policy object"
                    ));
                }
                let owner = self.palw_model_line_role(state, &payload.line_id, "owner")?;
                let message = kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_message_v1(
                    self.palw_network_domain_v2(),
                    &payload.line_id,
                    payload.sequence,
                    payload.policy.as_ref(),
                );
                self.palw_improvement_check_bond_signature(
                    state,
                    &owner,
                    &message,
                    signature,
                    kaspa_consensus_core::palw_improve_policy_v1::PALW_IMPROVE_POLICY_MLDSA87_CONTEXT,
                    "a policy object",
                )?;
            }
            // RFC-0004 (spec 17 §17.11.1): a sponsor's deposit is bound to its carrier's sink (the
            // extraction walk and isolation); the fold credits a governed line's pool or refuses it,
            // and P-B1 pays a refusal back.
            Obj::ImprovementPoolFunded { .. } => {
                if !self.palw_improvement_at(point.daa_score) {
                    return Err(format!(
                        "{} is refused: palw_improvement_v1 is not in force at this block (RFC-0004)",
                        "a pool deposit"
                    ));
                }
            }
            // RFC-0004 (spec 17 §17.10.1): a rollback, signed by its filer bond's key.
            Obj::LineageHeadRolledBack { payload, filer, signature } => {
                if !self.palw_improvement_at(point.daa_score) {
                    return Err(format!("{} is refused: palw_improvement_v1 is not in force at this block (RFC-0004)", "a rollback"));
                }
                let message = kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_rollback_message_v1(
                    self.palw_network_domain_v2(),
                    payload,
                );
                self.palw_improvement_check_bond_signature(
                    state,
                    filer,
                    &message,
                    signature,
                    kaspa_consensus_core::palw_improve_policy_v1::PALW_IMPROVE_ROLLBACK_MLDSA87_CONTEXT,
                    "a rollback",
                )?;
            }
            // RFC-0004 A4 (spec 17 §17.6): a bond-signed material object, signed under the material
            // context over its tag, the network, the payload and the signing bond. Everything else —
            // the line, the epoch, the references, the money — is the fold's.
            Obj::HardCaseSubmitted { payload, submitter, signature } => {
                self.palw_improvement_material_is_signed(state, point, 71, payload.as_ref(), submitter, signature, "a hard case")?;
            }
            Obj::SetterSetCommitted { payload, setter, signature } => {
                self.palw_improvement_material_is_signed(state, point, 73, payload.as_ref(), setter, signature, "a setter set")?;
            }
            Obj::DatasetRegistered { payload, registrant, signature } => {
                self.palw_improvement_material_is_signed(state, point, 76, payload.as_ref(), registrant, signature, "a dataset")?;
            }
            Obj::TeachingArtifactCommitted { payload, teacher, signature } => {
                self.palw_improvement_material_is_signed(
                    state,
                    point,
                    77,
                    payload.as_ref(),
                    teacher,
                    signature,
                    "an artifact commitment",
                )?;
            }
            // RFC-0004 §10 (PALW-MIP-19): the opt-in is its job's committer's — the bond of the
            // free-prompt claim it names, whose recorded job the fold compares with the pin.
            Obj::DataUseOptIn { payload, signature } => {
                let claim = state
                    .claim(&payload.claim)
                    .ok_or_else(|| "a data-use opt-in names a claim this chain does not have".to_string())?;
                let committer = claim.bond;
                self.palw_improvement_material_is_signed(
                    state,
                    point,
                    72,
                    payload.as_ref(),
                    &committer,
                    signature,
                    "a data-use opt-in",
                )?;
            }
            // Reveals are unsigned: each opens a commitment only its maker's salt opens (the fold's).
            Obj::SetterSetRevealed { .. }
            | Obj::SetterKeysRevealed { .. }
            | Obj::HardCaseKeyRevealed { .. }
            | Obj::TeachingArtifactRevealed { .. } => {
                if !self.palw_improvement_at(point.daa_score) {
                    return Err("a reveal is refused: palw_improvement_v1 is not in force at this block (RFC-0004)".to_string());
                }
            }
            // RFC-0004 §9 (PALW-MIP-18): a licence, signed by its rights holder's own ML-DSA-87 key.
            Obj::TeacherLicenceRegistered { payload, signature } => {
                if !self.palw_improvement_at(point.daa_score) {
                    return Err(
                        "a teacher licence is refused: palw_improvement_v1 is not in force at this block (RFC-0004)".to_string()
                    );
                }
                let message = kaspa_consensus_core::palw_improve_material_v1::palw_improve_material_message_v1(
                    79,
                    &self.palw_network_domain_v2(),
                    payload.as_ref(),
                    None,
                );
                if !Self::verify_mldsa87_with_context_bool(
                    &payload.rights_holder_key,
                    message.as_byte_slice(),
                    signature,
                    kaspa_consensus_core::palw_improve_material_v1::PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1,
                ) {
                    return Err("a teacher licence carries a signature its rights holder's key does not verify".to_string());
                }
            }
            // RFC-0004 §6 (A5, PALW-MIP-8, PALW-MIP-15): the submitter's signature, then the
            // acceptance half — the declarations' form, the parent, and the artifact's admission (a
            // composite sized in the composite form: the walk hands one a block to this gate).
            Obj::CandidateSubmitted { payload, submitter, signature } => {
                if !self.palw_improvement_at(point.daa_score) {
                    return Err("a candidate is refused: palw_improvement_v1 is not in force at this block (RFC-0004)".to_string());
                }
                self.palw_improvement_candidate_is_signed(state, payload.as_ref(), submitter, signature)?;
                let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                    return Err("a candidate on a network with no V2 bundle".to_string());
                };
                let rules = kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeAdmissionV1 {
                    court: self.palw_kary_court_active_at(point.daa_score),
                    carriable: kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court),
                    work_cap: kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
                };
                state
                    .improvement_candidate_acceptance_v1(payload, rules)
                    .map_err(|why| format!("candidate {} is not admissible: {why} (RFC-0004 §6)", payload.class_id))?;
            }
            _ => unreachable!("improvement validator received another object family"),
        }
        Ok(())
    }
}
