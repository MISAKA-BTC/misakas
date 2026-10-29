//! Object authentication and admission for the existing reporting family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_reporting_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
                // **ADR-0152 v3.1 R-3 (S-7): a reporter's commitment is authorised by the reporter
                // bond's own key, and by nothing else** — the lock a capability declaration carries,
                // for its reason: a bond key is a public outpoint, and a commitment filed under
                // somebody else's bond would take one of that bond's 64 open slots (and, revealed,
                // that bond's reward) without its owner. Signed over the network, the commitment
                // and the reporter (`palw_reporter_commit_message_v1`) under the new ML-DSA-87
                // context. The FENCE is checked here as well as in the fold: below
                // `Params::palw_rcore_plus` the object is dropped by name, as the v22 skeleton
                // dropped it, and the fold refuses it by name. Everything else — Active, the
                // floor, the 64-per-bond cap, a rooted duplicate — is the fold's, and the
                // rehearsal drops what the fold refuses.
                Obj::ReporterCommitted { commitment, reporter, signature } => {
                    if !self.palw_rcore_plus_at(point.daa_score) {
                        return Err(format!(
                            "{} is declared by the v22 layout (ADR-0152 R-core+) and refused until its owner lands it",
                            kaspa_consensus_core::palw_state_v2::palw_rcore_object_name_v1(object).unwrap_or("an R-core+ object")
                        ));
                    }
                    let record = state
                        .bond(reporter)
                        .ok_or_else(|| format!("a reporter commitment names bond {reporter:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_state_v2::palw_reporter_commit_message_v1(
                        &kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        commitment,
                        reporter,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        &message,
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_REPORTER_COMMIT_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("reporter commitment {commitment} is not signed by bond {reporter:?}'s registered key"));
                    }
                }
                // **ADR-0152 v3.1 R-3/R-4 (S-7): a reveal carries no signature** — anyone may carry
                // it; the salt is the secret and the commitment binds the reporter. Past the fence
                // the fold judges it whole (a pending reward, its window, the rooted commitment,
                // committed strictly before the conviction, not the accused, beating the best) and
                // the rehearsal drops what it refuses. Below the fence it is dropped by name.
                Obj::ReporterRevealed { .. } => {
                    if !self.palw_rcore_plus_at(point.daa_score) {
                        return Err(format!(
                            "{} is declared by the v22 layout (ADR-0152 R-core+) and refused until its owner lands it",
                            kaspa_consensus_core::palw_state_v2::palw_rcore_object_name_v1(object).unwrap_or("an R-core+ object")
                        ));
                    }
                }
                Obj::ObjectiveOffence { kind, accused, evidence_id, evidence } => {
                    if *kind == kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PrefixExportRefuted {
                        if !state_params.claim_local_settlement_active_at(point.daa_score) {
                            return Err("source export court below its claim-local fence".into());
                        }
                        if evidence.is_empty()
                            || evidence.len() as u64 > kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES
                            || kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != *evidence_id {
                            return Err("source export evidence byte bound or digest differs".into());
                        }
                        // Authoritative native decode and source court run in the ordered
                        // rehearsal/canonical fold. Preserve Resource errors there instead
                        // of reducing a local capacity failure to this String refusal.
                        return Ok(());
                    }
                    if let Some(reference) =
                        kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_reference_v1(*kind, evidence)
                        && (!self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score))
                            || !kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_collection_open_v1(
                                state,
                                &reference,
                                point.daa_score,
                            ))
                    {
                        return Err("rooted staged conviction is outside its accepting-block fence or collection clock".into());
                    }
                    if !self.palw_objective_offence_at(point.daa_score) {
                        return Err("an objective offence is not armed on this network (ADR-0144 §9)".into());
                    }
                    // **ADR-0152 v3.1: kinds 5 and 6 are records the fold writes, never filed** —
                    // refused by name on every network before any evidence is read, as the fold
                    // refuses them, permanently (the skeleton review's F1).
                    if kind.is_fold_recorded_only_v1() {
                        return Err(kaspa_consensus_core::palw_offence_v1::PalwOffenceVerifyError::KindNotFileable(
                            kind.not_fileable_reason_v1(),
                        )
                        .to_string());
                    }
                    let record = state
                        .bond(accused)
                        .ok_or_else(|| "an objective offence names a PALW bond this chain does not have".to_string())?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    // **ADR-0152 v2 F2 (`Params::palw_offence_attribution`): a false `Valid` has ONE
                    // route past the fence, judged here and in the fold by one adjudicator.** The V1
                    // `PanelFalseValid` is refused by name: its evidence convicts on
                    // `job_id == claim_id`, a fixed point no real block-lane or free-prompt claim
                    // reaches, while a receipt signed by hand in the V2 format still clears it for the
                    // state-bound contradictions (`ConflictingPermit` names no claim at all).
                    // `PanelFalseValidV2` is judged by `palw_check_panel_false_valid_v2` with the
                    // signature half the fold does not run — the receipt verified under THIS chain's
                    // domain and the key the accused bond registered, in the V2 or V3 form it was
                    // licensed in — and a (seat, claim) already convicted is refused, since the fold
                    // would carry it as a no-op. Whether the seat still holds the lock it is charged
                    // is the fold's question, asked by the per-object rehearsal that follows, so a
                    // refusal there drops the object too. Below the fence the V2 kind does not exist
                    // and everything after this block is the gate as it was.
                    if self.palw_offence_attribution_at(point.daa_score) {
                        use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, PalwOffenceVerifyError};
                        match kind {
                            PalwOffenceKindV1::PanelFalseValid => {
                                return Err(PalwOffenceVerifyError::SupersededOnThisNetwork.to_string());
                            }
                            PalwOffenceKindV1::PanelFalseValidV2 => {
                                if evidence.is_empty() {
                                    return Err(PalwOffenceVerifyError::EvidenceEmpty.to_string());
                                }
                                if kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != *evidence_id {
                                    return Err(PalwOffenceVerifyError::EvidenceIdMismatch.to_string());
                                }
                                // ADR-0160 F-B: a batched receipt convicts only past the batch fence
                                // (the fold asks the same predicate).
                                if !state_params.capacity_batch_active_at(point.daa_score)
                                    && kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_evidence_is_windowed_v1(
                                        evidence,
                                    )
                                {
                                    return Err("a Windowed receipt below palw_capacity_batch_licence (ADR-0160 F-B)".to_string());
                                }
                                // R6-CELL round 4: the coverage closure's mirrored height (None on every shipped network).
                                // RFC06-4: and consumer v2's, so past it this path judges the claim with its router binding
                                // exactly as the claim gate, the court and the fold do.
                                let finding =
                                    kaspa_consensus_core::palw_offence_attribution_v1::palw_check_panel_false_valid_blame_v2(
                                        state,
                                        accused,
                                        evidence,
                                        state_params.fp_decode_rules_at(point.daa_score),
                                        // F7's reporter slot: empty until its own fence arms it.
                                        false,
                                        self.palw_identity_rules_v1(point.daa_score),
                                        Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidSigCheckV1 {
                                            chain_domain: domain,
                                            seat_pubkey: &record.pubkey,
                                            seat_active: matches!(
                                                record.status,
                                                kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active
                                                    | kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }
                                            ),
                                            verify: &Self::verify_mldsa87_with_context_bool,
                                        }),
                                        // Lane LG14-A (RFC-0014 §7.4): the fold's session rule at this block.
                                        self.palw_session_rule_at(point.daa_score),
                                        kaspa_consensus_core::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::DirectCarrier,
                                        kaspa_consensus_core::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(state_params),
                                        true,
                                    )
                                    .map_err(|e| e.to_string())?;
                                let offence_id = kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(
                                    &accused.0,
                                    &finding.target.claim_id,
                                );
                                if state.consumed_offence(&offence_id).is_some() {
                                    return Err(format!(
                                        "bond {accused:?}'s false Valid on claim {} is already convicted: one offence per seat and claim",
                                        finding.target.claim_id
                                    ));
                                }
                                return Ok(());
                            }
                            // **ADR-0152 v3.1 J-4 (F1): the executor refuted.** The same adjudicator
                            // the fold runs — no receipt and no signature: the evidence is objective
                            // and names nobody but the claim's own executor — and a claim already
                            // refuted is refused here, since the fold would carry it as a no-op.
                            PalwOffenceKindV1::ExecutorRefuted => {
                                if evidence.is_empty() {
                                    return Err(PalwOffenceVerifyError::EvidenceEmpty.to_string());
                                }
                                if kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != *evidence_id {
                                    return Err(PalwOffenceVerifyError::EvidenceIdMismatch.to_string());
                                }
                                // RFC06-4: past consumer v2, the same closure predicate as kind 3 (below it, unchanged).
                                let finding = kaspa_consensus_core::palw_offence_attribution_v1::palw_check_executor_refuted_blame_v2(
                                    state,
                                    accused,
                                    evidence,
                                    state_params.fp_decode_rules_at(point.daa_score),
                                    false,
                                    self.palw_identity_rules_v1(point.daa_score),
                                    self.palw_session_rule_at(point.daa_score),
                                    kaspa_consensus_core::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::DirectCarrier,
                                    kaspa_consensus_core::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(state_params),
                                )
                                .map_err(|e| e.to_string())?;
                                let offence_id =
                                    kaspa_consensus_core::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(
                                        &accused.0,
                                        &finding.target.claim_id,
                                    );
                                if state.consumed_offence(&offence_id).is_some() {
                                    return Err(format!(
                                        "claim {}'s executor is already refuted: one offence per claim",
                                        finding.target.claim_id
                                    ));
                                }
                                return Ok(());
                            }
                            // **RFC-0002 Phase F: an IR executor's identity conviction** — kind 4's gate over
                            // the IR adjudicator: past `palw_tir_v1` (the walk dropped it below), the same
                            // no-receipt, no-signature reading, and one offence per claim.
                            PalwOffenceKindV1::TirIdentityMismatch => {
                                if !self.palw_tir_at(point.daa_score) {
                                    return Err(
                                        "an IR identity conviction is refused: palw_tir_v1 is not in force (RFC-0002)".to_string()
                                    );
                                }
                                if evidence.is_empty() {
                                    return Err(PalwOffenceVerifyError::EvidenceEmpty.to_string());
                                }
                                if kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != *evidence_id {
                                    return Err(PalwOffenceVerifyError::EvidenceIdMismatch.to_string());
                                }
                                let finding =
                                    kaspa_consensus_core::palw_offence_attribution_v1::palw_check_tir_identity_mismatch_at_v1(
                                        state,
                                        accused,
                                        evidence,
                                        false,
                                        self.palw_identity_rules_v1(point.daa_score),
                                        self.palw_session_rule_at(point.daa_score),
                                    )
                                    .map_err(|e| e.to_string())?;
                                let offence_id = kaspa_consensus_core::palw_offence_attribution_v1::palw_tir_identity_offence_id_v1(
                                    &accused.0,
                                    &finding.target.claim_id,
                                );
                                if state.consumed_offence(&offence_id).is_some() {
                                    return Err(format!(
                                        "claim {}'s IR executor is already convicted of its identity: one offence per claim",
                                        finding.target.claim_id
                                    ));
                                }
                                return Ok(());
                            }
                            PalwOffenceKindV1::ExecutorEquivocation | PalwOffenceKindV1::CourtExecutorGuilty => {}
                            // Refused above; listed so a routing change that forgets them fails to compile.
                            PalwOffenceKindV1::DaDefault | PalwOffenceKindV1::CourtConviction | PalwOffenceKindV1::PrefixExportRefuted => {}
                        }
                    } else if matches!(
                        kind,
                        kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2
                            | kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted
                            | kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::TirIdentityMismatch
                    ) {
                        return Err(kaspa_consensus_core::palw_offence_v1::PalwOffenceVerifyError::AttributionDormant.to_string());
                    }
                    // R6-CELL round 4: whether the coverage closure governs the accused claim (a `PanelFalseValid`'s named claim
                    // accepted at or past the closure's mirrored height); `false` on every shipped network and for every other kind.
                    // RFC06-4: past consumer v2 the mode carries the claim's router binding (the claim gate's and the court's
                    // predicate); below it this is the round-4 `bool` exactly (`for_blame_path(.., true)`).
                    let closure_mirrors = kaspa_consensus_core::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(state_params);
                    let closure_inputs = if matches!(kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValid) {
                        borsh::from_slice::<kaspa_consensus_core::palw_offence_v1::PalwPanelFalseValidEvidenceV1>(evidence)
                            .ok()
                            .and_then(|p| {
                                kaspa_consensus_core::palw_cell_coverage_closure_v1::palw_blame_closure_inputs_v1(
                                    state,
                                    &p.claim_id,
                                    closure_mirrors,
                                )
                            })
                    } else {
                        None
                    };
                    let cell_closure = closure_inputs.as_ref().map_or(
                        kaspa_consensus_core::palw_cell_coverage_closure_v1::PalwCellClosureModeV1::Off,
                        |(accepted, row)| {
                            kaspa_consensus_core::palw_cell_coverage_closure_v1::PalwCellClosureModeV1::for_blame_path(
                                closure_mirrors,
                                *accepted,
                                row.as_ref(),
                                true,
                            )
                        },
                    );
                    kaspa_consensus_core::palw_offence_v1::palw_verify_objective_offence_closure_v1(
                        *kind,
                        &accused.0,
                        evidence_id,
                        evidence,
                        &record.pubkey,
                        matches!(
                            record.status,
                            kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active
                                | kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }
                        ),
                        domain.as_byte_slice(),
                        // **ADR-0084 U-08: the RULESET's step ladder, not the executor's.** A
                        // `PanelFalseValid` whose contradiction is a step refutation is adjudicated
                        // inside that call, and walking a shallower ladder than the class was
                        // admitted at refuses honest evidence — on a network carrying the held 2M row
                        // (step space ~2^37.6) against the default 2^22 that is EVERY structural
                        // refutation of that class, so a seat that voted Valid on a visible lie could
                        // not be convicted through this route at all.
                        self.palw_max_step_leaf_count_v1(),
                        Self::verify_mldsa87_with_context_bool,
                        cell_closure,
                    )
                    .map_err(|e| e.to_string())?;
                    if let kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValid = kind {
                        let payload: kaspa_consensus_core::palw_offence_v1::PalwPanelFalseValidEvidenceV1 =
                            borsh::from_slice(evidence).map_err(|_| "PanelFalseValid evidence does not decode".to_string())?;
                        let (execution_root, artifact_root, class_id, executor_bond) =
                            if let Some(claim) = state.claim(&payload.claim_id) {
                                let artifact = state.class(&claim.class_id).map(|c| c.artifact_root).unwrap_or_default();
                                (claim.execution_root, artifact, claim.class_id, claim.bond)
                            } else if let Some(row) = state.panel_liability(&payload.claim_id) {
                                let artifact = state.class(&row.class_id).map(|c| c.artifact_root).unwrap_or_default();
                                (row.execution_root, artifact, row.class_id, row.executor_bond)
                            } else {
                                return Err("PanelFalseValid names neither a live claim nor a liability row".into());
                            };
                        // **2026-09-24 DoS audit review: the equivocation must be the EXECUTOR's.**
                        // `palw_verify_objective_offence_v1` above checked the two attestations under
                        // `payload.executor_pubkey` and the bond the carriage names — both supplied by
                        // the evidence, so a fresh key convicted any seat whose `Valid` receipt was
                        // public. Past the fence the carriage must accuse this claim's executor bond
                        // and the key must be the one that bond registered; the fold asks the same
                        // question (`bind_panel_false_valid`), so the object is dropped either way.
                        if self.palw_audit_2026_09_23_at(point.daa_score)
                            && let kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::ExecutorEquivocation(carriage) =
                                &payload.contradiction
                        {
                            if carriage.accused_bond_outpoint != executor_bond.0 {
                                return Err("PanelFalseValid's equivocation does not accuse the claim's executor bond".into());
                            }
                            let registered = state
                                .bond(&executor_bond)
                                .ok_or_else(|| "PanelFalseValid names an executor bond this chain no longer holds".to_string())?;
                            if registered.pubkey != payload.executor_pubkey {
                                return Err("PanelFalseValid's equivocation is not signed under the executor bond's key".into());
                            }
                        }
                        let ladder = state.class_step_ladder_v1(&class_id, 64);
                        kaspa_consensus_core::palw_offence_v1::palw_panel_contradiction_convicts_execution_closure_v1(
                            &payload.contradiction,
                            execution_root,
                            artifact_root,
                            ladder,
                            cell_closure,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
            _ => unreachable!("reporting validator received another object family"),
        }
        Ok(())
    }
}
