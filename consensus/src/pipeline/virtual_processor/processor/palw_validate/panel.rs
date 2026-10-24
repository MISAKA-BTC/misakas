//! Object authentication and admission for the existing panel family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_trap_reveal_classified_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        reveal: &kaspa_consensus_core::palw_mesh_v1::PalwTrapRevealedV1,
    ) -> Result<(), PalwObjectAdmissionFaultV1> {
        use kaspa_consensus_core::{palw_mesh_v1 as mesh, palw_state_v2::PalwStateV2Error};
        if !params.audit_mesh_active_at(point.daa_score) {
            return Err(mesh::PalwMeshErrorV1::Dormant("palw_audit_mesh_v1").to_string().into());
        }
        mesh::palw_trap_revealed_shape_v1(reveal).map_err(|why| why.to_string())?;
        let paid = state.paid_trap_target_checked_v1(params, &reveal.claim, point.daa_score).map_err(|why| {
            match why {
                PalwStateV2Error::NativeResource(why) => PalwObjectAdmissionFaultV1::Resource(why),
                other => PalwObjectAdmissionFaultV1::Semantic(other.to_string()),
            }
        })?;
        match paid {
            Some(paid) => state.paid_trap_revealed_admissible_v1(&paid, reveal, point.daa_score).map_err(str::to_string)?,
            None => state.mesh_trap_revealed_admissible_v2(reveal, point.daa_score, params.audit_1004_active_at(point.daa_score))
                .map_err(|why| why.to_string())?,
        }
        mesh::palw_trap_revealed_verify_v1(
            state,
            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                self.network_id_bytes.as_slice(), Some(self.genesis.hash),
            ),
            reveal, Self::verify_mldsa87_with_context_bool,
        ).map_err(|why| why.to_string())?;
        Ok(())
    }

    pub(super) fn palw_validate_panel_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
                // **RFC-0007 Part I: a verification vertex** (tag 91). The fence; the vertex's admissibility (its shape, its clock,
                // a registered bond, the first of its `(seat, round)`); then the one ML-DSA-87 signature over the whole round, under
                // the seat bond's registered key. Every leaf is the fold's: a leaf that does not count is ignored, so a seat can
                // only lose by saying something wrong.
                Obj::VerificationVertexV1 { vertex } => {
                    if !state_params.vertex_active_at(point.daa_score) {
                        return Err(kaspa_consensus_core::palw_vertex_v1::PalwVertexErrorV1::Dormant.to_string());
                    }
                    kaspa_consensus_core::palw_vertex_v1::palw_vertex_admissible_v1(state, vertex, point.daa_score)
                        .map_err(|e| e.to_string())?;
                    // An audit leaf rides a vertex only where `palw_audit_mesh_v1` is in force (RFC-0007 Part IV.1).
                    if let Some(index) = kaspa_consensus_core::palw_vertex_v1::palw_vertex_has_audit_leaf_v1(&vertex.leaves)
                        && !state_params.audit_mesh_active_at(point.daa_score)
                    {
                        return Err(kaspa_consensus_core::palw_vertex_v1::PalwVertexErrorV1::AuditedLeafNotArmed { index }.to_string());
                    }
                    kaspa_consensus_core::palw_vertex_v1::palw_vertex_verify_signature_v1(
                        state,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &vertex.seat_bond,
                        vertex.round,
                        vertex.signed_daa,
                        vertex.leaves.len(),
                        vertex.leaves_root,
                        &vertex.signature,
                        Self::verify_mldsa87_with_context_bool,
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **RFC-0007 Part IV.1: a trap commitment** (tag 93): the fence, the stateful admission (a registered Active setter drawn
                // by the slot lottery with no trap open and the deposit free), then the setter's one ML-DSA-87 signature.
                Obj::TrapCommittedV1 { trap } => {
                    if !state_params.audit_mesh_active_at(point.daa_score) {
                        return Err(kaspa_consensus_core::palw_mesh_v1::PalwMeshErrorV1::Dormant("palw_audit_mesh_v1").to_string());
                    }
                    kaspa_consensus_core::palw_mesh_v1::palw_trap_committed_shape_v1(trap).map_err(|e| e.to_string())?;
                    state.mesh_trap_committed_admissible_v1(trap, point.daa_score).map_err(|e| e.to_string())?;
                    kaspa_consensus_core::palw_mesh_v1::palw_trap_committed_verify_v1(
                        state,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        trap,
                        Self::verify_mldsa87_with_context_bool,
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **RFC-0007 Part IV.1: a trap's reveal** (tag 94): the fence, the stateful admission (it opens this setter's
                // commitment about its own audited claim, inside the reveal window — or, past the dormant `palw_paid_claim_trap_v1`,
                // a paid V2 base claim's trap: RFC-0007 §4.1 T6), then the setter's signature.
                Obj::TrapRevealedV1 { reveal } => {
                    self.palw_validate_trap_reveal_classified_v1(state, state_params, point, reveal)
                        .map_err(|why| why.to_string())?;
                }
                // **RFC-0010: a certified Panel epoch output** (tag 120): the fence, and the proof's bound (the lifecycle shape check
                // already holds it). Whether it verifies against THIS branch is the fold's, which drops one that does not and lets the
                // block stand — a bad carried proof never invalidates a block, so nothing stateful is asked here.
                Obj::PanelBeaconProofV3 { proof } => {
                    if !state_params.panel_v3_rule_at(point.daa_score) {
                        return Err(
                            "a permissionless-Panel certified output before palw_permissionless_panel_v1 is in force".to_string()
                        );
                    }
                    if proof.proof.len() > kaspa_consensus_core::palw_permissionless_panel_v1::MAX_BEACON_PROOF_BYTES_V1 as usize {
                        return Err("a certified Panel epoch output exceeds the proof bound (RFC-0010)".to_string());
                    }
                }
                // **RFC-0007 §I.6: two vertices of one `(seat, round)` with different roots** (tag 92) — admissible evidence
                // (one seat, one round, two roots, provable still, not yet convicted) and BOTH signatures under the seat's
                // registered key. No court: two signatures over one round are the whole proof.
                Obj::VertexEquivocationV1 { evidence } => {
                    if !state_params.vertex_active_at(point.daa_score) {
                        return Err(kaspa_consensus_core::palw_vertex_v1::PalwVertexErrorV1::Dormant.to_string());
                    }
                    kaspa_consensus_core::palw_vertex_v1::palw_vertex_equivocation_admissible_v2(
                        state,
                        evidence,
                        point.daa_score,
                        state_params.audit_1004_active_at(point.daa_score),
                    )
                    .map_err(|e| e.to_string())?;
                    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    for header in [&evidence.a, &evidence.b] {
                        kaspa_consensus_core::palw_vertex_v1::palw_vertex_verify_signature_v1(
                            state,
                            network_domain,
                            &header.seat_bond,
                            header.round,
                            header.signed_daa,
                            header.leaf_count as usize,
                            header.leaves_root,
                            &header.signature,
                            Self::verify_mldsa87_with_context_bool,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                // **ADR-0152 v3.1 §6, the v22 skeleton: tags 55–56 are declared, not landed.**
                // Dropped by name on every network, testnet-12 included, before anything else is
                // read — the fold refuses the same objects (`RcoreObjectNotLanded`), so the gate and
                // the fold agree, and the block carrying one stands. Each owner replaces this arm
                // with its acceptance rule (M3: 55, below; S-5: 56).
                //
                // **M3 (ADR-0152 DA-4): tag 55 past `palw_rcore_plus`.** The DA court's fence, the
                // ruleset's close ceiling on the answer, and the discloser's ML-DSA-87 over
                // `palw_da_disclosure_message_v4` under the DA-disclosure-v4 context, verified against the
                // DISCLOSER's registered key — any locked signer may answer (X7), so the signer is the
                // bond the object names, and whether that bond is liable on the claim is the fold's
                // (`DaDiscloserNotLiable`). Below the fence it stays refused by name, as before.
                Obj::MaterialDisclosedV2 { claim, unit, answer, discloser, signature }
                    if state_params.rcore_plus_active_at(point.daa_score) =>
                {
                    // RFC-0002 Phase F: an IR class's answer is dropped by name below `palw_tir_v1`,
                    // exactly as an older build skips the payload it cannot decode (A-2).
                    if answer.is_tir_v1() && !self.palw_tir_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: an IR answer is refused: palw_tir_v1 is not in force at this block (RFC-0002 Phase F)"
                        ));
                    }
                    if (answer.is_pipeline_da_v1() || unit.is_pipeline_step_v1()) && !self.palw_pipeline_da_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: a pipeline answer is refused: palw_improvement_v1 is not in force at this block (RFC-0004 Phase F)"
                        ));
                    }
                    if (answer.is_tir_shard_v1() || unit.is_tir_shard_v1()) && !self.palw_tir_shard_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: a step-run answer is refused: palw_tir_shard_v1 is not in force at this block (RFC-0006)"
                        ));
                    }
                    if !self.palw_da_court_at(point.daa_score) {
                        return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a data-availability answer on a network with no V2 court parameters".to_string())?;
                    // One count for this gate and for the responder that builds the answer
                    // (`palw_da_answer_object_v1`, P2-7).
                    let bytes = kaspa_consensus_core::palw_da_rcore_v1::palw_da_answer_bytes_v1(answer);
                    if bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim}'s answer is {bytes} bytes, above this ruleset's {}-byte close ceiling (DA-8)",
                            court.max_close_bytes()
                        ));
                    }
                    let record =
                        state.bond(discloser).ok_or_else(|| format!("an answer names bond {discloser:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_da_rcore_v1::palw_da_disclosure_message_v4(
                        &self.palw_network_domain_v2(),
                        claim,
                        unit,
                        &kaspa_consensus_core::palw_da_rcore_v1::palw_da_answer_digest_v1(answer),
                        discloser,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_da_rcore_v1::PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("claim {claim}'s answer is not signed by the bond it names"));
                    }
                }
                Obj::MaterialDisclosedV2 { .. } | Obj::PanelUnavailableQuorum { .. } => {
                    return Err(format!(
                        "{} is declared by the v22 layout (ADR-0152 R-core+) and refused until its owner lands it",
                        kaspa_consensus_core::palw_state_v2::palw_rcore_object_name_v1(object).unwrap_or("an R-core+ object")
                    ));
                }
                Obj::PanelBound { claim, anchor, seats } => {
                    let claim_record = state.claim(claim).ok_or_else(|| format!("panel names unknown claim {claim}"))?;
                    // RFC-0010: a claim accepted under the permissionless rule is bound by the fold, never by an object — a
                    // lane-A binding of it would be a second, operator-anchored answer to a question the engine answers.
                    if matches!(
                        state_params.panel_claim_rule_v1(claim_record.accepted_daa),
                        kaspa_consensus_core::palw_permissionless_panel_v1::PanelClaimRuleV1::PermissionlessV3
                    ) {
                        return Err(format!(
                            "claim {claim} is bound by the permissionless Panel's fold (RFC-0010); a lane-A binding of it is refused"
                        ));
                    }
                    let anchor_fact = self
                        // The REDRAW's base, or the second panel's anchor is derived for a slot
                        // `validate_panel_bound_v2` no longer expects (it moved to
                        // `bind_base_daa()`), and every revived claim fails `AnchorMismatch`.
                        .palw_v2_anchor_fact_for_claim_v1(point.block, claim_record, state_params, panel_params)
                        .ok_or_else(|| format!("no anchor exists yet for claim {claim} on this chain"))?;
                    // ADR-0065 D1. The fence is read at the ANCHOR's DAA, not at this block's,
                    // and the sibling assembler does the same: the panel is a pure function of the
                    // claim, so the rule that decides it has to be one too. Resolving at
                    // `point.daa_score` would make the derived panel change from block to block
                    // around the fence height, and a `PanelBound` that missed the block it was
                    // built for would be refused as a mismatch rather than accepted late.
                    kaspa_consensus_core::palw_panel_v2::validate_panel_bound_v2_with_tir_shard_v1(
                        state,
                        panel_params,
                        state_params,
                        point,
                        claim,
                        &anchor_fact,
                        *anchor,
                        seats,
                        self.palw_bond_maturity_window_at(state, anchor_fact.anchor_daa),
                        // ADR-0071 SA-3, at the ANCHOR for the D1 reason one line up: the panel is
                        // a pure function of the claim, so the rule that narrows it must be one
                        // too — and the assembler below resolves it at the same point.
                        self.palw_capability_bound_at(anchor_fact.anchor_daa),
                        // C-02 (deep fence) and ADR-0124 (the panel economy): the whole draw policy,
                        // resolved at the ANCHOR for the same purity reason — the assembler resolves
                        // it at the same point, so build and validate recompute one identical panel.
                        // The Valid-lock question (route-matrix #3) is the BINDING block's, as the
                        // assembler asks it.
                        kaspa_consensus_core::palw_panel_v2::PalwPanelDrawPolicyV1 {
                            valid_lock: Self::palw_panel_valid_lock_of_v1(
                                state,
                                state_params,
                                &self.palw_transition_extras_for(point),
                                claim,
                                claim_record,
                                point.daa_score,
                                panel_params.seat_count() as usize,
                            ),
                            ..self.palw_panel_draw_policy_at(anchor_fact.anchor_daa)
                        },
                        // ADR-0100 Decision 4: the same one-place decision the binding made.
                        self.palw_stratified_shard_count(state, &claim_record.class_id, anchor_fact.anchor_daa),
                        // RFC-0006: and the IR class's layer-shard plan, decided at the same anchor.
                        self.palw_tir_shard_count(state, &claim_record.class_id, anchor_fact.anchor_daa),
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **The receipt quorum, verified where the design always said it was** (audit
                // M-01). `PalwConsensusObjectV2::ReceiptLicensed`'s own doc said it carried "the
                // receipt set the acceptance layer validated" — and this match had no arm for it,
                // so nothing ever did. `ProducerDefaulted { receipts: [] }` from any stranger
                // reached `slash_silent_seats` (every seat charged `claim.reserved`) and
                // `void_and_slash` (the producer's bond debited), on a transaction that carried no
                // signature at all. `validate_receipt_quorum_v2` was written, tested, and had no
                // caller anywhere in the tree.
                //
                // The quorum's DIRECTION is checked against the object, not just its existence: a
                // `Licensed` quorum cannot default a producer and an `Unavailable` quorum cannot
                // license one. The majority invariant (`2·quorum > seat_count`) is what makes the
                // two provably disjoint, so exactly one of these objects can ever be acceptable
                // for a given receipt set.
                Obj::ReceiptLicensed { claim, receipts } | Obj::ProducerDefaulted { claim, receipts } => {
                    let quorum = kaspa_consensus_core::palw_panel_v2::validate_receipt_quorum_v2_with_economy(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        receipts,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                        // ADR-0124 Decision 2: the supplementary door, at the carrying block's DAA.
                        self.palw_panel_economy_active_at(point.daa_score),
                        // ADR-0147: a bought class's licence carries its outsider's `Valid`.
                        self.palw_admission_independence_daa(),
                    )
                    .map_err(|e| format!("claim {claim}'s receipt set does not carry a quorum: {e}"))?;
                    use kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2 as Q;
                    match (object, quorum) {
                        (Obj::ReceiptLicensed { .. }, Q::Licensed { .. })
                        | (Obj::ProducerDefaulted { .. }, Q::ProducerUnavailable { .. }) => {}
                        // ADR-0124 Decision 2: a supplementary set rides the licensing object's kind
                        // and credits seats on a claim already licensed; it can default nobody.
                        (Obj::ReceiptLicensed { .. }, Q::Supplementary { .. }) => {}
                        (Obj::ProducerDefaulted { .. }, Q::Supplementary { .. }) => {
                            return Err(format!("claim {claim} is already licensed; a supplementary receipt set defaults nobody"));
                        }
                        (Obj::ReceiptLicensed { .. }, Q::ProducerUnavailable { .. }) => {
                            return Err(format!("claim {claim} is licensed by a quorum that says the producer withheld"));
                        }
                        (Obj::ProducerDefaulted { .. }, Q::Licensed { .. }) => {
                            return Err(format!("claim {claim} is defaulted by a quorum that verified its trace"));
                        }
                        _ => unreachable!("the outer arm matched exactly these two object kinds"),
                    }
                }
                Obj::ReceiptLicensedV2 { claim, receipts } => {
                    if !self.palw_verification_v2_at(point.daa_score) {
                        return Err(format!("claim {claim}: a segment-scoped receipt set below Verification V2's fence (ADR-0133)"));
                    }
                    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    // **ADR-0152 SR-10: the V3 supplementary door.** Past `palw_rcore_plus` the same
                    // object on a claim already licensed is a supplementary set of V3 receipts, not a
                    // licence; it is verified as one (routed on the claim's phase, as the fold routes
                    // it). Below the fence nothing is routed and a licensed claim refuses it as before.
                    let supplementary = state_params.rcore_plus_active_at(point.daa_score)
                        && state.claim(claim).is_some_and(|record| {
                            matches!(record.phase, kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::ReceiptLicensed { .. })
                        });
                    let quorum = if supplementary {
                        kaspa_consensus_core::palw_panel_v2::validate_supplementary_receipts_v3(
                            state,
                            state_params,
                            point,
                            network_domain,
                            claim,
                            receipts,
                            Self::verify_mldsa87_with_context_bool,
                        )
                        .map_err(|e| format!("claim {claim}'s supplementary V3 receipts are refused: {e}"))?
                    } else {
                        kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                            state,
                            panel_params,
                            state_params,
                            point,
                            network_domain,
                            claim,
                            receipts,
                            Self::verify_mldsa87_with_context_bool,
                            self.palw_unavailable_abstains_at(point.daa_score),
                            // ADR-0147: coverage by the class's own seats is still the class's own seats.
                            self.palw_admission_independence_daa(),
                        )
                        .map_err(|e| format!("claim {claim}'s segment receipts do not license: {e}"))?
                    };
                    match quorum {
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::Licensed { .. } if !supplementary => {}
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::Supplementary { .. } if supplementary => {}
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::Licensed { .. } => {
                            return Err(format!("claim {claim} is already licensed; a supplementary set licenses nothing"));
                        }
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::ProducerUnavailable { .. } => {
                            return Err(format!("claim {claim} is licensed by a quorum that says the producer withheld"));
                        }
                        kaspa_consensus_core::palw_panel_v2::PalwReceiptQuorumV2::Supplementary { .. } => {
                            return Err(format!("claim {claim}: supplementary receipts ride the V1 object"));
                        }
                    }
                }
                // **ADR-0160 F-B: the batch licence** (tag 59). Past `palw_capacity_batch_licence`
                // only: every root's signature once, then each live entry through the single
                // licence's own validator with its paths standing for the signatures
                // (`palw_validate_batch_licence_v1`). An inert entry is skipped here and in the fold
                // by one predicate; any other fault drops the object, the block standing.
                Obj::ReceiptLicensedBatchV1 { roots, entries } => {
                    // The fence through the bundle's mirror (`Params::sync_palw_capacity_verify`), which
                    // `validate_palw_v2` holds equal to it: the fold reads the same copy.
                    if !state_params.capacity_batch_active_at(point.daa_score) {
                        return Err(kaspa_consensus_core::palw_batch_licence_v1::PalwBatchLicenceErrorV1::Dormant.to_string());
                    }
                    kaspa_consensus_core::palw_batch_licence_v1::palw_validate_batch_licence_v1(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        self.palw_verification_v2_at(point.daa_score),
                        roots,
                        entries,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                        self.palw_admission_independence_daa(),
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **ADR-0160 F-Q: an audit receipt batch** (tag 60). Past `palw_capacity_audit_door` only
                // (the bundle's mirror): the auditor's ML-DSA-87 signature under its genesis-registered key
                // (lane A's operator rule holds the operators' keys), then the fold's own refusals asked
                // first (`palw_audit_receipt_batch_refusal_v1`), so a miner never carries a block-invalid
                // batch; the entries the door does not keep are the fold's skips.
                Obj::AuditReceiptBatchV1 { auditor, entries, signature } => {
                    if !state_params.capacity_audit_active_at(point.daa_score) {
                        return Err("an audit receipt batch below palw_capacity_audit_door (ADR-0160 F-Q)".to_string());
                    }
                    let key = self
                        .palw_operator_anchor
                        .as_ref()
                        .and_then(|rule| rule.operators().find(|(bond, _)| *bond == auditor).map(|(_, key)| key.to_vec()))
                        .ok_or_else(|| format!("audit receipt batch: {auditor:?} is not an operator bond"))?;
                    let message = kaspa_consensus_core::palw_audit_door_v1::palw_audit_receipt_batch_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        auditor,
                        entries,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &key,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_audit_door_v1::PALW_AUDIT_RECEIPT_V1_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("audit receipt batch: not signed by {auditor:?}'s registered key"));
                    }
                    if let Some(why) = kaspa_consensus_core::palw_audit_door_v1::palw_audit_receipt_batch_refusal_v1(
                        state,
                        state_params,
                        auditor,
                        entries,
                    ) {
                        return Err(format!("audit receipt batch by {auditor:?}: {why}"));
                    }
                }
                // **ADR-0164 F-M1: riders** (tag 95). Past `palw_capacity_multi_claim` only (the bundle's mirror): between one and 64
                // riders, each of the current attempt version on this network, carrying the challenge its lead and its index derive
                // (it has no header position), signed by its executor key. Everything stateful — the lead, its window, the bond's room,
                // the budget, the admission of each rider at its share — is the fold's (step 4b′), which skips a refused batch.
                Obj::AttemptRidersV1 { lead, riders } => {
                    use kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION;
                    if !state_params.capacity_riders_active_at(point.daa_score) {
                        return Err("an attempt-riders object below palw_capacity_multi_claim (ADR-0164 F-M1)".to_string());
                    }
                    if riders.is_empty() || riders.len() > kaspa_consensus_core::palw_capacity_s567_v1::PALW_RIDERS_MAX_V1 {
                        return Err(format!("riders of {lead}: between one and 64 riders a batch"));
                    }
                    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    // **Lane PA, C-F1 / B-F6 (`palw_audit_1004_v1`)**: the cheap state tests come before any ML-DSA verification (up to 64 of
                    // them) and before the fold's full-state checkpoint: the riders are of one registered bond, and each carries that
                    // bond's key (the fold's admission would refuse a stranger's key after the checkpoint and the verifications).
                    if state_params.audit_1004_active_at(point.daa_score) {
                        let bond = riders[0].attempt.executor_bond;
                        if riders.iter().any(|rider| rider.attempt.executor_bond != bond) {
                            return Err(format!("riders of {lead}: a batch is of one bond"));
                        }
                        let Some(record) = state.bond(&kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(bond)) else {
                            return Err(format!("riders of {lead}: the executor bond is not registered"));
                        };
                        if riders.iter().any(|rider| rider.attempt.executor_pubkey != record.pubkey) {
                            return Err(format!("riders of {lead}: a rider is not signed by its bond's registered key"));
                        }
                    }
                    for (index, envelope) in riders.iter().enumerate() {
                        envelope
                            .validate_shape_v2_at_version(PALW_ATTEMPT_V2_VERSION)
                            .map_err(|e| format!("rider {index} of {lead}: {e}"))?;
                        if envelope.attempt.network_domain != network_domain
                            || envelope.attempt.challenge
                                != kaspa_consensus_core::palw_capacity_s567_v1::palw_rider_challenge_v1(lead, index as u32)
                        {
                            return Err(format!("rider {index} of {lead}: not bound to this network, its lead and its index"));
                        }
                        envelope
                            .validate_signature_v2(|key, message, sig, context| {
                                Self::verify_mldsa87_with_context_bool(key, message, sig, context)
                            })
                            .map_err(|e| format!("rider {index} of {lead}: {e}"))?;
                    }
                }
                Obj::OptimisticLicensed { claim, receipts } => {
                    if !self.palw_verification_s2_at(point.daa_score) {
                        return Err(format!("claim {claim}: an optimistic licence below Verification S2's fence (ADR-0133)"));
                    }
                    let Some(panel) = state.panel(claim) else {
                        return Err(format!("claim {claim}: an optimistic licence names a claim with no panel"));
                    };
                    let seats: Vec<_> = panel.seats.iter().map(|s| s.bond).collect();
                    kaspa_consensus_core::palw_optimistic_licence_v2::palw_optimistic_receipts_license_v2(
                        panel.anchor,
                        *claim,
                        &seats,
                        receipts,
                    )
                    .map_err(|e| format!("claim {claim}'s optimistic receipts do not license: {e}"))?;
                    let coverage = kaspa_consensus_core::palw_panel_v2::validate_receipt_coverage_v2(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        receipts,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                        self.palw_admission_independence_daa(),
                    );
                    // Signatures and seats must still be real. Below the quorum the door does not require
                    // coverage; at or past it the set must cover (`CoverageShort` is refused). One
                    // predicate, which the node's selection (`palw_select_optimistic_licence_v2`) asks of
                    // every set it offers.
                    if let Err(e) = &coverage
                        && !kaspa_consensus_core::palw_optimistic_licence_v2::palw_optimistic_coverage_admits_v2(&coverage)
                    {
                        return Err(format!("claim {claim}'s optimistic receipts do not verify: {e}"));
                    }
                }
                Obj::TirShardReceiptLicensed { part } => {
                    if !self.palw_tir_shard_at(point.daa_score) {
                        return Err(format!("claim {}: layer-sharded panels are not armed on this network (RFC-0006)", part.claim));
                    }
                    kaspa_consensus_core::palw_panel_v2::validate_tir_shard_part_v1(
                        state,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        part,
                        Self::verify_mldsa87_with_context_bool,
                    )
                    .map_err(|e| format!("claim {} shard {}: the part does not license its shard: {e}", part.claim, part.shard))?;
                }
                Obj::ShardReceiptLicensed { part } => {
                    if !self.palw_shard_licensing_at(point.daa_score) {
                        return Err(format!("claim {}: per-shard licensing is not armed on this network (ADR-0100)", part.claim));
                    }
                    kaspa_consensus_core::palw_panel_v2::validate_shard_receipt_part_v1(
                        state,
                        panel_params,
                        state_params,
                        point,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        part,
                        Self::verify_mldsa87_with_context_bool,
                        self.palw_unavailable_abstains_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {} shard {}: the part does not carry a quorum: {e}", part.claim, part.shard_index))?;
                }
                // **ADR-0062 SA-2: the disclosure is signed by the CLAIM's bond, and bounded by the
                // ruleset's close ceiling before a single Merkle path is walked.**
                //
                // The signature is the producer's, not the carrier's — which is what makes SA-3's
                // permissionless carriage safe: anyone may carry it, and nobody may write it. The
                // ceiling is applied here for the reason ADR-0049 Decision C's costs are (audit
                // H-03): a bound checked only at class admission is a bound nothing enforces where
                // it is actually spendable.
                Obj::MaterialDisclosed { claim, event_index, disclosure, signature } => {
                    if !self.palw_da_court_at(point.daa_score) {
                        return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                    }
                    if state_params.rcore_plus_active_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: MaterialDisclosed is retired past palw_rcore_plus; MaterialDisclosedV2 answers (DA-1)"
                        ));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a data-availability disclosure on a network with no V2 court parameters".to_string())?;
                    let disclosure_bytes = borsh::to_vec(disclosure).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                    if disclosure_bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim}'s disclosure is {disclosure_bytes} bytes, above this ruleset's {}-byte close ceiling",
                            court.max_close_bytes()
                        ));
                    }
                    let record =
                        state.claim(claim).ok_or_else(|| format!("a disclosure names claim {claim} this chain does not have"))?;
                    let producer = state
                        .bond(&record.bond)
                        .ok_or_else(|| format!("claim {claim}'s producing bond is not in this chain's registry"))?;
                    let message = kaspa_consensus_core::palw_state_v2::palw_da_disclosure_message_v3(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        *event_index,
                        &kaspa_consensus_core::palw_state_v2::palw_da_disclosure_digest_v1(disclosure),
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &producer.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_DA_DISCLOSURE_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("claim {claim}'s disclosure is not signed by the bond that produced it"));
                    }
                }
            _ => unreachable!("panel validator received another object family"),
        }
        Ok(())
    }
}
