//! Object authentication and admission for the existing execution family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_execution_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            // **RFC-0008 v2: a work-session root declaration** (tag 130): the fence, its shape (a valid plan partition, set roots,
            // canonical executors), then the ONE ML-DSA-87 signature of the root claim's bond over the declaration bound to this
            // network. The claim's phase, prefix, job and quotas are the fold's.
            Obj::ExecWorkRootOpenedV2 { declaration } => {
                if !state_params.exec_v2_active_at(point.daa_score) {
                    return Err(kaspa_consensus_core::palw_work_slice_v2::PalwWorkRootRefusalV2::Dormant.to_string());
                }
                declaration.validate_shape().map_err(|e| e.to_string())?;
                kaspa_consensus_core::palw_work_slice_v2::palw_work_root_verify_signature_v2(
                    state,
                    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    ),
                    declaration,
                    Self::verify_mldsa87_with_context_bool,
                )
                .map_err(|e| e.to_string())?;
            }
            Obj::ExecOriginRecordedV1 { witness } => {
                state.exec_v2_origin_witness_admissible_v1(state_params, point.daa_score, witness).map_err(|e| e.to_string())?;
            }
            Obj::FreePromptCommitted { .. } => {}
            // **RFC-0003 §I.4: a tensor claim's commitment** is a free-prompt commitment of job version 10:
            // the acceptance walk built it only past both fences, from a payload whose signature it verified
            // under the key the job carries, and the fold derives the rest (the class, the work, the capacity).
            Obj::GenTensorCommitted { .. } => {}
            // **ADR-0078: a derivation is authorised by the key it declares, on this chain.**
            // The ride list proved a signature is present and the shape is the object's; here
            // the signature is verified under the declared executor key, over the object's own
            // message under its own context, and an object naming another network's domain is
            // refused before any state is read. Whether the declared key is the claim's bond
            // key is the transition's comparison (`DerivedSignerIsNotTheExecutor`).
            Obj::DerivedArtifactV1 { object, signature } => {
                let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                    self.network_id_bytes.as_slice(),
                    Some(self.genesis.hash),
                );
                if object.network_domain != network_domain {
                    return Err(format!("the derivation of claim {} names another network's domain", object.claim_id));
                }
                let message = kaspa_consensus_core::palw_derived_v1::palw_derived_message_v1(object);
                if !Self::verify_mldsa87_with_context_bool(
                    &object.executor_pubkey,
                    message.as_byte_slice(),
                    signature,
                    kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_MLDSA87_CONTEXT,
                ) {
                    return Err(format!("the derivation of claim {} is not signed by the executor key it declares", object.claim_id));
                }
            }
            _ => unreachable!("execution validator received another object family"),
        }
        Ok(())
    }
}
