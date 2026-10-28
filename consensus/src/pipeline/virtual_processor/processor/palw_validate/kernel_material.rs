//! Object authentication and admission for the existing kernel_material family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_kernel_material_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            // **G14 lane D: a chunk that COMPLETES a group carrying a kernel route object** is where that object's signature is
            // checked (the fold trusts the acceptance layer for it, as it does for every directly carried one): the assembled
            // inner is judged exactly as the direct `KernelRouteV1` arm judges it, and an inner the fence or the signature
            // refuses drops the completing chunk. Every other chunk is the transition's — and so is one whose inner kind's owning
            // fence is not in force here, which the live build cannot decode (A2U: `palw_kernel_chunk_inner` answers `None`).
            Obj::ObjectChunk { .. } => match self.palw_kernel_chunk_inner(state, object, point.daa_score) {
                Some(Obj::KernelRouteV1 { bytes, signer, signature }) => {
                    self.palw_kernel_route_object_is_signed(state, point.daa_score, &bytes, &signer, &signature)?;
                }
                // A binding's refutation delivered in chunks: judged as the direct object is, on the assembled whole.
                Some(Obj::ArtifactBindingChallengedV1 { v2_class, kernel_param_root, challenger, proof, signature }) => {
                    let payload = borsh::to_vec(&(v2_class, kernel_param_root, proof.as_ref())).map_err(|e| e.to_string())?;
                    self.palw_onboarding_signature_ok(state, point.daa_score, 105, &challenger, &payload, &signature)?;
                }
                // Onboarding P0: conformance evidence (or its refutation) delivered in chunks, judged on the assembled whole.
                Some(Obj::ConformanceEvidenceV1 { v2_class, action, signer, signature }) => {
                    let payload = borsh::to_vec(&(v2_class, action.as_ref())).map_err(|e| e.to_string())?;
                    self.palw_onboarding_signature_ok(state, point.daa_score, 109, &signer, &payload, &signature)?;
                }
                // DA16: a provider's answer delivered in chunks, judged on the assembled whole.
                Some(Obj::ProviderAnswerV1 { subject, provider, unit, answer, signature }) => {
                    let payload = borsh::to_vec(&(subject, unit, answer.as_ref())).map_err(|e| e.to_string())?;
                    self.palw_provider_court_signature_ok(state, point.daa_score, 152, &provider, &payload, &signature)?;
                }
                _ => {}
            },
            // **RFC-0001 §2.10 (ADR-0163): an adapter class's listing** — the lister's signature, then the same
            // acceptance half a composite candidate meets (the parent's family, the composite rule, the
            // reference, every terminal close carriable in the composite form).
            // **G14 lane D (tag 110): a kernel route object** — the fence, the signer's ML-DSA-87 signature over
            // `H(network ‖ signer ‖ bytes)`, then the kernel's strict decode. A well-signed encoding the ledger then refuses is the
            // fold's drop, not an invalid block.
            Obj::KernelRouteV1 { bytes, signer, signature } => {
                self.palw_kernel_route_object_is_signed(state, point.daa_score, bytes, signer, signature)?;
            }
            // **G14 (tag 113, C4 F-C4R3-03): a chunk of the route's own chunk lane** — the fence, the opener's Active bond and its
            // ML-DSA-87 signature over the chunk; at the chunk that COMPLETES its group, the assembled object's own signature,
            // judged exactly as the direct object's arm judges it (an inner the fence or the signature refuses drops the completing
            // chunk, and the group's deposit is forfeited at its TTL). The group's rows, target and deposit are the fold's.
            Obj::KernelRouteChunkV1 { chunk, signature } => {
                self.palw_kernel_chunk_is_signed(state, point.daa_score, chunk, signature)?;
                // Native target 6 debits the original protected assembly
                // allowance inside the fold before reconstructing or decoding
                // the body. Every part already authenticates its exact bytes,
                // group, target and opener. The fold binds the reconstructed
                // inner signer/action/seal to that opener and target; repeating
                // assembly here would bypass the paid failed-attempt charge.
                if matches!(
                    chunk.target,
                    kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkTargetV1::FundedProofV11 { .. }
                        | kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkTargetV1::StageProofV14 { .. }
                ) {
                    return Ok(());
                }
                match state.kernel_route().and_then(|k| k.chunk_completion_v1(chunk)) {
                    Some(Obj::KernelRouteV1 { bytes, signer, signature }) => {
                        self.palw_kernel_route_object_is_signed(state, point.daa_score, &bytes, &signer, &signature)?;
                    }
                    Some(Obj::ArtifactBindingChallengedV1 { v2_class, kernel_param_root, challenger, proof, signature }) => {
                        let payload = borsh::to_vec(&(v2_class, kernel_param_root, proof.as_ref())).map_err(|e| e.to_string())?;
                        self.palw_onboarding_signature_ok(state, point.daa_score, 105, &challenger, &payload, &signature)?;
                    }
                    // OPV-BOOT #1: conformance evidence (tag 109), judged as the direct object is.
                    Some(Obj::ConformanceEvidenceV1 { v2_class, action, signer, signature }) => {
                        let payload = borsh::to_vec(&(v2_class, action.as_ref())).map_err(|e| e.to_string())?;
                        self.palw_onboarding_signature_ok(state, point.daa_score, 109, &signer, &payload, &signature)?;
                    }
                    _ => {}
                }
            }
            // **G14 lane D phase 3 (tags 104-107): the onboarding objects** — the fence, an Active signer, the signer's signature over
            // the object's payload; the rows, the proofs and the V2 class's own facts are the fold's.
            Obj::ArtifactBoundV1 { v2_class, kernel_param_root, signer, signature } => {
                let payload = borsh::to_vec(&(v2_class, kernel_param_root)).map_err(|e| e.to_string())?;
                self.palw_onboarding_signature_ok(state, point.daa_score, 104, signer, &payload, signature)?;
            }
            Obj::ArtifactBindingChallengedV1 { v2_class, kernel_param_root, challenger, proof, signature } => {
                let payload = borsh::to_vec(&(v2_class, kernel_param_root, proof.as_ref())).map_err(|e| e.to_string())?;
                self.palw_onboarding_signature_ok(state, point.daa_score, 105, challenger, &payload, signature)?;
            }
            Obj::KernelBoundV1 { v2_class, kernel_class, challenge_policy_id, signer, signature } => {
                let payload = borsh::to_vec(&(v2_class, kernel_class, challenge_policy_id)).map_err(|e| e.to_string())?;
                self.palw_onboarding_signature_ok(state, point.daa_score, 106, signer, &payload, signature)?;
            }
            Obj::ConformanceCommittedV1 { commitment, signer, signature } => {
                let payload = borsh::to_vec(commitment.as_ref()).map_err(|e| e.to_string())?;
                self.palw_onboarding_signature_ok(state, point.daa_score, 107, signer, &payload, signature)?;
            }
            // **Onboarding P0 (tag 109): conformance evidence** — the fence, an Active signer, its signature over `(class, action)`;
            // whether it is the registrant's evidence or a refutation, and what it proves, are the fold's.
            Obj::ConformanceEvidenceV1 { v2_class, action, signer, signature } => {
                let payload = borsh::to_vec(&(v2_class, action.as_ref())).map_err(|e| e.to_string())?;
                self.palw_onboarding_signature_ok(state, point.daa_score, 109, signer, &payload, signature)?;
            }
            // **Lane DA16 (tags 150–153): the provider court** — the fence, an Active signer, its signature over the payload; the
            // leases, units and verdicts are the fold's.
            Obj::MonetaryMaterialEnrolledV1 { enrollment, signature } => {
                if state_params.monetary_material_at_v1(point.daa_score).is_none() {
                    return Err("monetary material enrollment is dormant".into());
                }
                let payload = enrollment.payload();
                self.palw_provider_court_signature_ok(state, point.daa_score, 240, &enrollment.terms.sponsor, &payload, signature)?;
                for offer in &enrollment.offers {
                    self.palw_provider_court_signature_ok(state, point.daa_score, 240, &offer.provider, &payload, &offer.signature)?;
                }
            }
            Obj::MonetaryMaterialReplacedV1 { replacement, signature } => {
                if state_params.monetary_material_at_v1(point.daa_score).is_none() {
                    return Err("monetary material replacement is dormant".into());
                }
                let payload = replacement.payload();
                self.palw_provider_court_signature_ok(
                    state,
                    point.daa_score,
                    241,
                    &replacement.enrollment.terms.sponsor,
                    &payload,
                    signature,
                )?;
                for offer in &replacement.enrollment.offers {
                    self.palw_provider_court_signature_ok(state, point.daa_score, 241, &offer.provider, &payload, &offer.signature)?;
                }
            }
            Obj::MonetaryMaterialActionV1 { action, signer, signature } => {
                if state_params.monetary_material_at_v1(point.daa_score).is_none() {
                    return Err("monetary material action is dormant".into());
                }
                if let kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::Release {
                    root,
                    consent,
                    tail_block,
                } = action.as_ref()
                {
                    let row = state.monetary_material_row_v1(*root).ok_or("material release has no enrollment")?;
                    if row.consent != *consent || row.tail_block != Some(*tail_block) {
                        return Err("material release substitutes its consent/tail".into());
                    }
                    let block_data = self.ghostdag_store.get_data(point.block).map_err(|e| e.to_string())?;
                    let pruning_point = self.pruning_point_store.read().pruning_point().map_err(|e| e.to_string())?;
                    let finality = self.depth_manager.calc_finality_point(&block_data, pruning_point);
                    if !self.reachability_service.is_chain_ancestor_of(*tail_block, block_data.selected_parent)
                        || !self.reachability_service.is_chain_ancestor_of(*tail_block, finality)
                    {
                        return Err("monetary material tail is not finalized on the carrying block's selected chain".into());
                    }
                } else {
                    if let kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::RecoveryV2(recovery) =
                        action.as_ref()
                    {
                        if state_params.material_release_recovery_at_v2(point.daa_score).is_none() {
                            return Err("release recovery own V2 gate is dormant".into());
                        }
                        let payload = recovery.payload();
                        for provider in recovery.provider_signatures() {
                            self.palw_provider_court_signature_ok(
                                state,
                                point.daa_score,
                                242,
                                &provider.provider,
                                &payload,
                                &provider.signature,
                            )?;
                        }
                        if let kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2::FundedEnroll {
                            monetary,
                            sponsor_signature,
                            ..
                        } = recovery.as_ref()
                        {
                            let payload = monetary.payload();
                            self.palw_provider_court_signature_ok(
                                state,
                                point.daa_score,
                                240,
                                &monetary.terms.sponsor,
                                &payload,
                                sponsor_signature,
                            )?;
                            for offer in &monetary.offers {
                                self.palw_provider_court_signature_ok(
                                    state,
                                    point.daa_score,
                                    240,
                                    &offer.provider,
                                    &payload,
                                    &offer.signature,
                                )?;
                            }
                        }
                        if let kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2::Replace {replacement,sponsor_signature,..}
                                | kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2::ReplaceBeforeV4 {replacement,sponsor_signature,..} = recovery.as_ref() {
                                let payload=replacement.payload();
                                self.palw_provider_court_signature_ok(state, point.daa_score, 241, &replacement.enrollment.terms.sponsor, &payload, sponsor_signature)?;
                                for offer in &replacement.enrollment.offers {
                                    self.palw_provider_court_signature_ok(state, point.daa_score, 241, &offer.provider, &payload, &offer.signature)?;
                                }
                            }
                    }
                    let payload = borsh::to_vec(action.as_ref()).map_err(|e| e.to_string())?;
                    self.palw_provider_court_signature_ok(state, point.daa_score, 242, signer, &payload, signature)?;
                }
            }
            Obj::ProviderLeaseV1 { subject, reserved, serve_until_daa, provider, signature } => {
                let payload = borsh::to_vec(&(subject, reserved, serve_until_daa)).map_err(|e| e.to_string())?;
                self.palw_provider_court_signature_ok(state, point.daa_score, 150, provider, &payload, signature)?;
            }
            Obj::ProviderChallengeV1 { subject, provider, unit, valid_until_daa, challenger, signature } => {
                let payload = borsh::to_vec(&(subject, provider, unit, valid_until_daa)).map_err(|e| e.to_string())?;
                self.palw_provider_court_signature_ok(state, point.daa_score, 151, challenger, &payload, signature)?;
            }
            Obj::ProviderAnswerV1 { subject, provider, unit, answer, signature } => {
                let payload = borsh::to_vec(&(subject, unit, answer.as_ref())).map_err(|e| e.to_string())?;
                self.palw_provider_court_signature_ok(state, point.daa_score, 152, provider, &payload, signature)?;
            }
            Obj::DaTransferV1 { claim, producer, signature } => {
                let payload = borsh::to_vec(claim).map_err(|e| e.to_string())?;
                self.palw_provider_court_signature_ok(state, point.daa_score, 153, producer, &payload, signature)?;
            }
            // (tag 111): the seat's signature over the receipt's signing message, under the receipt context.
            Obj::KernelConstraintReceiptV1 { receipt, signature } => {
                if !self.palw_kernel_route_at(point.daa_score) {
                    return Err("a kernel constraint receipt is refused: palw_probabilistic_constraints_v1 is not in force at this block (G14)".to_string());
                }
                let record = state
                    .kernel_route()
                    .and_then(|k| k.bond_key_of(&receipt.seat_bond))
                    .and_then(|key| state.bond(&key))
                    .ok_or_else(|| "a kernel receipt is signed by a bond the route never assigned".to_string())?;
                if !matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                    return Err("a kernel receipt is signed by a bond that is not Active".to_string());
                }
                if !Self::verify_mldsa87_with_context_bool(
                    &record.pubkey,
                    &receipt.signing_message(),
                    signature,
                    misaka_palw_kernel::receipt::CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1,
                ) {
                    return Err("a kernel receipt carries a signature the seat's key does not verify".to_string());
                }
            }
            _ => unreachable!("kernel_material validator received another object family"),
        }
        Ok(())
    }
}
