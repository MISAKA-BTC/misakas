//! **`VirtualStateProcessor`: palw validate methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

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
                // commitment about its own audited claim, inside the reveal window), then the setter's signature.
                Obj::TrapRevealedV1 { reveal } => {
                    if !state_params.audit_mesh_active_at(point.daa_score) {
                        return Err(kaspa_consensus_core::palw_mesh_v1::PalwMeshErrorV1::Dormant("palw_audit_mesh_v1").to_string());
                    }
                    kaspa_consensus_core::palw_mesh_v1::palw_trap_revealed_shape_v1(reveal).map_err(|e| e.to_string())?;
                    state
                        .mesh_trap_revealed_admissible_v2(reveal, point.daa_score, state_params.audit_1004_active_at(point.daa_score))
                        .map_err(|e| e.to_string())?;
                    kaspa_consensus_core::palw_mesh_v1::palw_trap_revealed_verify_v1(
                        state,
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        reveal,
                        Self::verify_mldsa87_with_context_bool,
                    )
                    .map_err(|e| e.to_string())?;
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
                Obj::CourtOpened { session_id, claim, challenger_bond, space, space_size, signature } => {
                    // **ADR-0103 Decision 1: the held regime plays no bisection.** Refused here and
                    // in the fold, for the DA court's doubled-fence reason.
                    if self.palw_held_context_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: the held regime plays no bisection — accuse the leaf in one move (ADR-0103 Decision 1)"
                        ));
                    }
                    // The object now DECLARES the space, because the transition opens a ladder
                    // from it — but the ruleset still DECIDES it. H2's rule is unchanged: a space
                    // the accuser chose is a ladder depth the accuser chose, so a declaration that
                    // is not the catalog's own step-leaf count is refused here, before the
                    // transition ever sees it.
                    let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                    if *space != kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves
                        || *space_size != court.max_step_leaf_count()
                    {
                        return Err(format!(
                            "court {session_id} declares a {space_size}-wide {space:?} space; the ruleset's is {} step leaves",
                            court.max_step_leaf_count()
                        ));
                    }
                    kaspa_consensus_core::palw_court_v2::validate_court_opened_v2(
                        state,
                        state_params,
                        point,
                        session_id,
                        claim,
                        challenger_bond,
                        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
                        court.max_step_leaf_count(),
                        // The challenger's own signature over the session id (audit M-01). Without
                        // it every check above is a fact ABOUT the bond and none about who spoke
                        // for it, so anyone could prosecute under a stranger's identity — and the
                        // transition disarms the claim's final deadline while a session is open,
                        // which turns that into a freeze of an honest producer's claim.
                        signature,
                        Self::verify_mldsa87_with_context_bool,
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **ADR-0080 design A, W6: the declaration is a court move, and it is signed.**
                //
                // This arm refused every `CourtCloseDeclared` outright until W6 landed, and the
                // refusal was right for as long as it stood: a declaration pins every byte of a
                // close and asserts a verdict on behalf of one of the two bonds the session id
                // binds, the transition acts on it, and nothing proved the SIDE authorised it —
                // P0-9's hole in its original words, where either party could write the other's
                // move. Unsigned it was worse than that: a declaration is singular per
                // `(session, side)` for the life of the session and its failure to assemble is the
                // declarer's conviction, so one forged object would have voided an honest
                // producer's claim.
                //
                // `check_court_close_declaration_acceptance_v2` is the same split
                // `CourtDisclosed` and `CourtVerdictPosted` use: the transition reads WHICH bond
                // may declare, this layer reads whether that bond spoke — its registered key is in
                // the candidate state, and neither party names its own.
                Obj::CourtCloseDeclared { session_id, side, count, chunk_digests, close_digest, verdict, signature } => {
                    // The ruleset's own carriage count, which the transition cannot ask for: it has
                    // no `PalwCourtParamsV2` and enforces only the bitmap's structural bound. On
                    // devnet this is 1, so the split path is refused there rather than engaged.
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a court close declaration on a network with no V2 court parameters".to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_close_declared_chunk_count_v2(session_id, *count, court)
                        .map_err(|e| e.to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_court_close_declaration_acceptance_v2(
                        state,
                        session_id,
                        *side,
                        *count,
                        chunk_digests,
                        close_digest,
                        *verdict,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                // **ADR-0080 design A, W7: the chunk that COMPLETES a group is adjudicated here.**
                //
                // A chunk carries no signature and needs none — the declaration already pinned
                // these bytes at this index, and the transition refuses anything else — so for
                // every chunk but the last there is nothing for this layer to add.
                //
                // The last one is a court close. The split exists so that a close too wide for one
                // carrier can be filed at all, and the whole point of design A is that it reaches
                // the SAME verdict by the SAME function: `adjudicate_court_close_v2` over the
                // assembled proof, compared against the verdict the declaration announced, refused
                // in either direction — "an asserted conviction and an asserted acquittal are the
                // same lie", exactly as the one-carrier `CourtClosed` arm below says it.
                //
                // **What this arm does NOT refuse, and why that is not an omission.** Bytes that do
                // not hash to the declaration's own `close_digest`, or that do not decode to this
                // session's `CourtClosed`, are ADMITTED here — because a refused lifecycle object
                // is dropped with the block standing, which would leave the group intact and the
                // declarer sitting on its reserve until the backstop. Those two facts are
                // establishable from rooted state alone and are entirely the declarer's doing, so
                // the transition convicts on them in the block that carries the last chunk (W5's
                // "a failed declaration loses on its OWN side"). This layer only ever refuses what
                // it alone can see: whether the proof inside adjudicates.
                Obj::CourtCloseChunk { session_id, side, index, bytes } => {
                    if kaspa_consensus_core::palw_state_v2::palw_court_close_completes_a_group_v1(state, object)
                        && let Some(group) = state.court_close_group(session_id, *side)
                    {
                        let mut assembled = Vec::new();
                        for i in 0..group.count {
                            match (i == *index).then_some(bytes).or_else(|| group.chunks.get(&i)) {
                                Some(part) => assembled.extend_from_slice(part),
                                // Unreachable behind the predicate above, which is exactly the
                                // "every part present" test — asserted rather than assumed, because
                                // a hole here would adjudicate a truncated proof.
                                None => return Err(format!("court {session_id} completes with chunk {i} missing")),
                            }
                        }
                        let assembles =
                            kaspa_consensus_core::palw_state_v2::palw_court_close_chunk_digest_v1(&assembled) == group.close_digest;
                        let decoded = assembles
                            .then(|| borsh::from_slice::<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>(&assembled).ok())
                            .flatten()
                            // RFC-0002 Phase F: below `palw_tir_v1` an IR proof does not decode on an
                            // older build, so here it is bytes that do not decode as well — admitted,
                            // and the transition convicts the declarer exactly as the older build does.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_tir_v1() && !self.palw_tir_at(point.daa_score))
                            })
                            // RFC-0003: likewise a generative proof below `palw_gen_v1`.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_gen_v1() && !self.palw_gen_at(point.daa_score))
                            })
                            // The task-head profile: likewise a proof carrying a `Head` job below `palw_task_heads_v1`.
                            .filter(|object| {
                                !kaspa_consensus_core::palw_task_heads_v1::palw_object_needs_task_heads_v1(object)
                                    || self.palw_task_heads_at(point.daa_score)
                            })
                            // RFC-0004 A6: likewise an evaluation proof below `palw_improvement_v1`.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_eval_v1() && !self.palw_improvement_at(point.daa_score))
                            })
                            // ADR-0096 Decision 7: likewise a constrained decode proof below `palw_fp_decode_constraint`.
                            .filter(|object| {
                                !matches!(object, Obj::CourtClosed { proof, .. }
                                    if proof.is_constrained_decode_v1() && !self.palw_fp_decode_constraint_at(point.daa_score))
                            });
                        // Only when the bytes ARE this session's close does the adjudication run;
                        // anything else is the transition's conviction, not this layer's refusal.
                        if let Some(Obj::CourtClosed { session_id: decoded_session, verdict, proof }) = &decoded
                            && decoded_session == session_id
                            && *verdict == group.verdict
                        {
                            let court = self
                                .palw_court_params_v2
                                .as_ref()
                                .ok_or_else(|| "a court close on a network with no V2 court parameters".to_string())?;
                            let derived = kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
                                state,
                                session_id,
                                proof,
                                court,
                                self.palw_court_step_ladder_at(point.daa_score, court),
                                self.palw_prompt_ids_form_at(point.daa_score),
                                // ADR-0119 Decision 4: a fused site's rows open at the claim's
                                // ladder under the held regime.
                                self.palw_held_context_at(point.daa_score),
                                // ADR-0152 v3.1 addendum §4-bis.9: the court door.
                                self.palw_offence_attribution_at(point.daa_score),
                                // RFC-0001 §A: a decode-token close does not try an FP Job V4 claim.
                                self.palw_fp_decode_rules
                                    .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
                                    .map(|f| f.daa_score()),
                            )
                            .map_err(|e| e.to_string())?;
                            if derived != *verdict {
                                return Err(format!(
                                    "court {session_id}: the {} side declared {verdict:?} and the close it assembled adjudicates \
                                     {derived:?}",
                                    side.name()
                                ));
                            }
                        }
                    }
                }
                Obj::CourtDisclosed { session_id, disclosure, signature } => {
                    // The responder's rung, under the responder's key. Unsigned, a challenger
                    // could write the answers it wants to convict.
                    kaspa_consensus_core::palw_court_v2::check_court_disclosure_acceptance_v2(
                        state,
                        session_id,
                        disclosure,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                // ADR-0087 Decision 6: the market's two moves exist only past the fence, refused by
                // name before it (the drop-not-invalidate shape). A sell is signed by the key whose
                // id is the holder (M8), checked here where the verifier lives. The id is not the
                // payee: past the 2026-09-23 fence the fold pays the key's own address payload
                // (the 2026-09-23 Position route matrix, P-B4).
                Obj::ModelBuy { line_id, .. } => {
                    if !self.palw_model_market_active_at(point.daa_score) {
                        return Err(format!("a model buy of line {line_id} on a chain where the model market is not in force"));
                    }
                }
                // ADR-0090: the seed is the market's third move, under the same fence; its sink
                // binding is checked with the buy's (`palw_model_buy_binds_its_carrier_v1`).
                Obj::ModelSeed { line_id, .. } => {
                    if !self.palw_model_market_active_at(point.daa_score) {
                        return Err(format!("a model seed of line {line_id} on a chain where the model market is not in force"));
                    }
                }
                // **ADR-0152-adjacent (Activation Pool): a top-up exists only past the pool's fence**,
                // refused by name before it (the drop-not-invalidate shape; its MSK comes back through
                // P-B1 where the carrier is refunded). Its sink binding was checked at extraction
                // (`palw_activation_pool_binds_its_carrier_v1`), and an activation sink nothing binds
                // made the block invalid at isolation; the class, its status and the least top-up are
                // the fold's (`palw_activation_pool_admits_v1`).
                Obj::ActivationPoolFunded { class_id, .. } => {
                    if self.palw_activation_pool_at(point.daa_score).is_none() {
                        return Err(format!(
                            "a top-up of class {class_id}'s Activation Pool on a chain where the pool is not in force"
                        ));
                    }
                }
                Obj::ModelSell { line_id, holder, units_in, min_msk_out, held_units, not_after_daa, pubkey, signature } => {
                    if !self.palw_model_market_active_at(point.daa_score) {
                        return Err(format!("a model sell of line {line_id} on a chain where the model market is not in force"));
                    }
                    if kaspa_consensus_core::palw_model_market_v1::palw_model_holder_of_pubkey_v1(pubkey) != *holder {
                        return Err(format!("a model sell of line {line_id} is signed by a key that is not the holder's"));
                    }
                    // **ADR-0087 M8 (mainnet audit 2026-09-06, M-11): an authority with an end.**
                    // Past its own score the sell is refused; and a window longer than
                    // `PALW_MODEL_SELL_MAX_WINDOW_DAA_V1` is refused at any score, so "forever" is
                    // not expressible. Both compared against the ACCEPTING block's DAA, which is
                    // the point the holder's key was asked about.
                    match kaspa_consensus_core::palw_model_market_v1::palw_model_sell_window_v1(point.daa_score, *not_after_daa) {
                        kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::Live => {}
                        kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::Expired => {
                            return Err(format!(
                                "a model sell of line {line_id} expired at daa {not_after_daa}; this block is at {}",
                                point.daa_score
                            ));
                        }
                        kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::TooLong { daa } => {
                            return Err(format!(
                                "a model sell of line {line_id} claims authority for {daa} daa, over the {} the chain allows",
                                kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SELL_MAX_WINDOW_DAA_V1
                            ));
                        }
                    }
                    let message = kaspa_consensus_core::palw_model_market_v1::palw_model_sell_message_v1(
                        // **The network domain every other market/registry message already carries**
                        // (audit M-11). ADR-0088's ten objects all sign under it; this one did not,
                        // so one signature was good on any chain sharing the line id.
                        self.palw_network_domain_v2(),
                        line_id,
                        holder,
                        *units_in,
                        *min_msk_out,
                        *held_units,
                        *not_after_daa,
                    );
                    if !kaspa_txscript::verify_mldsa87_with_context(
                        pubkey,
                        &message,
                        signature,
                        kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SELL_MLDSA87_CONTEXT,
                    )
                    .unwrap_or(false)
                    {
                        return Err(format!("a model sell of line {line_id} carries a signature the holder's key does not verify"));
                    }
                }
                // ADR-0088 Decision 11: the registry's ten objects exist only past the fence, refused
                // by name before it. Each is attributed to a bond the fold names — the founder, the
                // line's developer, its owner, the proposer, the evaluator — and its signature is
                // checked HERE, against that bond's stored key, over the message the object's fields
                // spell; the fold then enforces referential integrity and the bounds.
                Obj::ModelLineFounded { class_id, name, founder, root, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!(
                            "a line founding on class {class_id} on a chain where the model registry is not in force"
                        ));
                    }
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_line_founded_message_v1(
                        self.palw_network_domain_v2(),
                        class_id,
                        name,
                        founder,
                        root,
                    );
                    self.palw_model_check_bond_signature(state, founder, &message, signature, "a line founding")?;
                }
                Obj::ModelVersionPublished {
                    line_id,
                    version,
                    root,
                    parent,
                    adopted_from,
                    runtime_hash,
                    dataset_commitment,
                    training_config_hash,
                    notes_hash,
                    preview,
                    signature,
                } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a version of line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        root,
                        *parent,
                        adopted_from.as_ref(),
                        runtime_hash.as_ref(),
                        dataset_commitment.as_ref(),
                        training_config_hash.as_ref(),
                        notes_hash.as_ref(),
                        *preview,
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a version")?;
                }
                Obj::ModelVersionPromoted { line_id, version, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a promotion on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_move_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        b"promote",
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a promotion")?;
                }
                Obj::ModelVersionWithdrawn { line_id, version, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a withdrawal on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_move_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        b"withdraw",
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a withdrawal")?;
                }
                Obj::ModelLineBenefitsDeclared { line_id, tiers, cadence_daa, expires_daa, signature } => {
                    // ADR-0095 §4.11 as corrected: its own fence, checked here as well as in the
                    // fold, so an object that cannot apply never rides in the first place.
                    if !self.palw_model_benefits_active_at(point.daa_score) {
                        return Err(format!(
                            "a benefits declaration for line {line_id} on a chain where the membership is not in force"
                        ));
                    }
                    // §4.1: the OWNER signs what the line promises, not the developer who ships it.
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_benefits_v1::palw_model_benefits_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        tiers,
                        *cadence_daa,
                        *expires_daa,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a benefits declaration")?;
                }
                Obj::ModelLineRolesSet { line_id, developer, maintainer, contributor_permille_of_leg, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a roles change on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_roles_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        developer.as_ref(),
                        maintainer.as_ref(),
                        *contributor_permille_of_leg,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a roles change")?;
                }
                Obj::ModelLineOwnerTransferred { line_id, new_owner, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a transfer of line {line_id} on a chain where the model registry is not in force"));
                    }
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_transfer_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        new_owner,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a transfer")?;
                }
                Obj::ModelLineRetired { line_id, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a retirement of line {line_id} on a chain where the model registry is not in force"));
                    }
                    let owner = self.palw_model_line_role(state, line_id, "owner")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_retire_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                    );
                    self.palw_model_check_bond_signature(state, &owner, &message, signature, "a retirement")?;
                }
                Obj::ModelProposalPosted { line_id, root, note_hash, by, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a proposal on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_proposal_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        root,
                        note_hash,
                        by,
                    );
                    self.palw_model_check_bond_signature(state, by, &message, signature, "a proposal")?;
                }
                Obj::ModelProposalClosed { line_id, proposal_id, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("a proposal close on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let developer = self.palw_model_line_role(state, line_id, "developer")?;
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_proposal_close_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        proposal_id,
                    );
                    self.palw_model_check_bond_signature(state, &developer, &message, signature, "a proposal close")?;
                }
                Obj::ModelEvaluationPosted { line_id, version, evaluator_id, score_permille, report_hash, by, signature } => {
                    if !self.palw_model_lines_active_at(point.daa_score) {
                        return Err(format!("an evaluation on line {line_id} on a chain where the model registry is not in force"));
                    }
                    let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_evaluation_message_v1(
                        self.palw_network_domain_v2(),
                        line_id,
                        *version,
                        evaluator_id,
                        *score_permille,
                        report_hash,
                        by,
                    );
                    self.palw_model_check_bond_signature(state, by, &message, signature, "an evaluation")?;
                }
                Obj::CourtVerdictPosted { session_id, verdict, signature } => {
                    kaspa_consensus_core::palw_court_v2::check_court_verdict_acceptance_v2(
                        state,
                        session_id,
                        verdict,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtClosed { session_id, verdict, proof } => {
                    // RFC-0002 Phase F: an IR close is dropped by name below `palw_tir_v1`, exactly
                    // as an older build skips the payload it cannot decode (A-2).
                    if proof.is_tir_v1() && !self.palw_tir_at(point.daa_score) {
                        return Err(format!(
                            "court {session_id}: an IR close is refused: palw_tir_v1 is not in force at this block (RFC-0002 Phase F)"
                        ));
                    }
                    // RFC-0003: a generative close likewise below `palw_gen_v1`.
                    if proof.is_gen_v1() && !self.palw_gen_at(point.daa_score) {
                        return Err(format!(
                            "court {session_id}: a generative close is refused: palw_gen_v1 is not in force at this block (RFC-0003)"
                        ));
                    }
                    // ADR-0096 Decision 7: a constrained decode close likewise below `palw_fp_decode_constraint`.
                    if proof.is_constrained_decode_v1() && !self.palw_fp_decode_constraint_at(point.daa_score) {
                        return Err(format!(
                            "court {session_id}: a constrained decode close is refused: palw_fp_decode_constraint is not in force at this block (ADR-0096)"
                        ));
                    }
                    // RFC-0004 A6: an evaluation close likewise below `palw_improvement_v1`.
                    if proof.is_eval_v1() && !self.palw_improvement_at(point.daa_score) {
                        return Err(format!(
                            "court {session_id}: an evaluation close is refused: palw_improvement_v1 is not in force at this block (RFC-0004)"
                        ));
                    }
                    // The close carries its proof now, so there is something to adjudicate. The
                    // node re-derives the verdict from the proof and compares: a declared verdict
                    // that its own proof does not produce is refused, in EITHER direction — an
                    // asserted conviction and an asserted acquittal are the same lie.
                    //
                    // `adjudicate_court_close_v2` refuses outright when the proof does not
                    // adjudicate (an out-of-catalog kernel, a non-canonical operand set, a
                    // binding naming another execution), so an unadjudicable object convicts
                    // nobody AND acquits nobody — P0-8's rule, on both sides.
                    //
                    // ADR-0049 Decision C, audit H-03: the ruleset's cost ceilings are applied to
                    // the OBJECT, before a single Merkle path is walked. They were checked only
                    // against a CLASS's geometry at admission — a bound the ruleset id commits to
                    // and that nothing enforced where it is actually spendable.
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a court close on a network with no V2 court parameters".to_string())?;
                    let derived = kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
                        state,
                        session_id,
                        proof,
                        court,
                        self.palw_court_step_ladder_at(point.daa_score, court),
                        self.palw_prompt_ids_form_at(point.daa_score),
                        // ADR-0119 Decision 4.
                        self.palw_held_context_at(point.daa_score),
                        // ADR-0152 v3.1 addendum §4-bis.9: the court door.
                        self.palw_offence_attribution_at(point.daa_score),
                        // RFC-0001 §A: a decode-token close does not try an FP Job V4 claim.
                        self.palw_fp_decode_rules
                            .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
                            .map(|f| f.daa_score()),
                    )
                    .map_err(|e| e.to_string())?;
                    if derived != *verdict {
                        return Err(format!("court {session_id} declares {verdict:?}; its own proof adjudicates {derived:?}"));
                    }
                }
                // -------------------------------------------------------------------------
                // ADR-0082 Decision 2/3 — the dissection's three moves.
                //
                // Two gates, in this order: the FENCE (a move on a network that never armed the
                // k-ary court is refused by name — the arm exists in the binary but the rule does
                // not exist on this chain), then the party's signature, the same split every
                // other court move uses.
                // -------------------------------------------------------------------------
                Obj::CourtAttnRootClaimed { session_id, root, arity, signature, .. }
                | Obj::CourtAttnRootClaimedAnchored { session_id, root, arity, signature, .. }
                | Obj::CourtAttnRootClaimedHeld { session_id, root, arity, signature, .. } => {
                    // ADR-0093 Decision 8: the anchored form exists only where its fence does. (The
                    // plain form's refusal past the fence needs the site, so the fold makes it.)
                    if matches!(object, Obj::CourtAttnRootClaimedAnchored { .. }) && !self.palw_attn_anchored_root_at(point.daa_score)
                    {
                        return Err(format!(
                            "session {session_id}: an anchored root claim before palw_attn_anchored_root is armed (ADR-0093 Decision 8)"
                        ));
                    }
                    // ADR-0152 §4-ter C2: the held form (tag 57) exists only past
                    // `palw_offence_attribution`, at the block's own DAA. Whether the class is an
                    // answerable held one, and whether the sub-roots root to the anchor, need the
                    // session and the site, so the fold makes those refusals; the signature and the
                    // arity are the same checks as the other two forms', below.
                    if matches!(object, Obj::CourtAttnRootClaimedHeld { .. }) && !self.palw_offence_attribution_at(point.daa_score) {
                        return Err(format!(
                            "session {session_id}: a held root claim before palw_offence_attribution is armed (ADR-0152 §4-ter)"
                        ));
                    }
                    kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    // **The declared arity must be the ruleset's own** (patch note 7). The object
                    // states it because the fold cannot derive it — the derivation reads the
                    // bundle — and this is the layer that holds the bundle, so this is the layer
                    // that refuses any other value. Without the comparison the responder would
                    // choose how many children it discloses per round, which is the move budget
                    // Z4 sizes the window against.
                    let derived = self
                        .palw_court_params_at(point.daa_score)
                        .ok_or_else(|| "a dissection move on a network with no V2 bundle".to_string())?
                        .map_err(|e| format!("session {session_id}: {e}"))?;
                    if *arity != derived.dissection_arity() {
                        return Err(format!(
                            "session {session_id}: the root claim declares dissection arity {arity}; this ruleset derives {}",
                            derived.dissection_arity()
                        ));
                    }
                    kaspa_consensus_core::palw_court_v2::check_court_attn_root_claim_acceptance_v2(
                        state,
                        session_id,
                        root,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtAttnDissected { session_id, round, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    kaspa_consensus_core::palw_court_v2::check_court_attn_round_acceptance_v2(
                        state,
                        session_id,
                        round,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtAttnChildChosen { session_id, choice, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_attn_move_is_admissible_v2(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    kaspa_consensus_core::palw_court_v2::check_court_attn_choice_acceptance_v2(
                        state,
                        session_id,
                        choice,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                // -------------------------------------------------------------------------
                // RFC-0002 Phase F, F7 — the IR history dissection's three moves (spec 04b §9.5).
                //
                // ADR-0082's gates: the FENCE (the k-ary court — `palw_tir_v1` dropped the object by
                // name above), the party's SIGNATURE, and for the root claim the arity against the
                // ruleset and the FINALIZE: the tile evaluated from the claimed totals must be the
                // committed one, a demand evaluation at the IR court's limits and this block's step
                // ladder — the same the close of its bottom is judged at — which is why the move
                // takes the block's adjudication slot (`palw_court_move_spends_the_slot_v1`).
                // -------------------------------------------------------------------------
                Obj::CourtTirRootClaimed { session_id, root, arity, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    let derived = self
                        .palw_court_params_at(point.daa_score)
                        .ok_or_else(|| "an IR dissection move on a network with no V2 bundle".to_string())?
                        .map_err(|e| format!("session {session_id}: {e}"))?;
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "an IR dissection move on a network with no V2 court parameters".to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_court_tir_root_claim_acceptance_v1(
                        state,
                        session_id,
                        root,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    // The claim's CLASS ladder, as every held court arm reads it (an IR class records
                    // none, so it is the network's; a missing session is refused by the check below).
                    let bare = self.palw_court_step_ladder_at(point.daa_score, court);
                    let ladder = state
                        .court_session(session_id)
                        .and_then(|session| state.claim(&session.claim))
                        .map_or(bare, |claim| state.class_step_ladder_v1(&claim.class_id, bare));
                    kaspa_consensus_core::palw_court_v2::check_court_tir_root_claim_admits_v1(
                        state,
                        session_id,
                        root,
                        *arity,
                        derived.dissection_arity(),
                        court,
                        ladder,
                        self.palw_prompt_ids_form_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                }
                // RFC-0003 (tag 69): the generative root claim — F7's gates over the generative
                // registry: the k-ary fence (`palw_gen_v1` dropped it by name below), the responder's
                // signature, the arity and the finalize at the court's limits.
                Obj::CourtGenRootClaimed { session_id, root, arity, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    let derived = self
                        .palw_court_params_at(point.daa_score)
                        .ok_or_else(|| "a generative dissection move on a network with no V2 bundle".to_string())?
                        .map_err(|e| format!("session {session_id}: {e}"))?;
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a generative dissection move on a network with no V2 court parameters".to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_court_gen_root_claim_acceptance_v1(
                        state,
                        session_id,
                        root,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_court_gen_root_claim_admits_v1(
                        state,
                        session_id,
                        root,
                        *arity,
                        derived.dissection_arity(),
                        court,
                        self.palw_prompt_ids_form_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                }
                // RFC-0004 A6: the evaluation root claim — the generative arm's gates over the claim's own evaluation
                // context: the improvement fence (the walk dropped it by name below), the k-ary court, the
                // responder's signature, the arity and the finalize at the court's limits.
                Obj::CourtEvalRootClaimed { session_id, root, arity, signature } => {
                    if !self.palw_improvement_at(point.daa_score) {
                        return Err(format!(
                            "session {session_id}: an evaluation root claim is refused: palw_improvement_v1 is not in force (RFC-0004)"
                        ));
                    }
                    kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    let derived = self
                        .palw_court_params_at(point.daa_score)
                        .ok_or_else(|| "an evaluation dissection move on a network with no V2 bundle".to_string())?
                        .map_err(|e| format!("session {session_id}: {e}"))?;
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "an evaluation dissection move on a network with no V2 court parameters".to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_court_eval_root_claim_acceptance_v1(
                        state,
                        session_id,
                        root,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    kaspa_consensus_core::palw_court_v2::check_court_eval_root_claim_admits_v1(
                        state,
                        session_id,
                        root,
                        *arity,
                        derived.dissection_arity(),
                        court,
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                }
                Obj::CourtTirDissected { session_id, round, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    kaspa_consensus_core::palw_court_v2::check_court_tir_round_acceptance_v1(
                        state,
                        session_id,
                        round,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::CourtTirChildChosen { session_id, choice, signature } => {
                    kaspa_consensus_core::palw_court_v2::palw_tir_dissection_move_is_admissible_v1(
                        object,
                        self.palw_kary_court_active_at(point.daa_score),
                    )
                    .map_err(|e| format!("session {session_id}: {e}"))?;
                    kaspa_consensus_core::palw_court_v2::check_court_tir_choice_acceptance_v1(
                        state,
                        session_id,
                        choice,
                        signature,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                Obj::ClassRegistered {
                    class_id,
                    share_permille,
                    admission,
                    // Checked by `palw_v2_class_registration_starts_at_the_chains_target`, below.
                    initial_target: _,
                    // Signed over by `palw_v2_class_registration_is_signed`, below.
                    activation_daa: _,
                    artifact_root: _,
                    slash_value_per_pwu: _,
                    pwu_rule: _,
                } => {
                    // **ADR-0049 Decision H: the gate, where the refusal used to be.**
                    //
                    // A class registration is the one object whose validity is an arithmetic fact
                    // about a GRAPH — that every kernel it reaches is adjudicable (ADR-0038 A4),
                    // that its longest job fits the ruleset's ladder, that prosecuting it costs
                    // what the ruleset allows (Decision C), and that its declared
                    // `pwu_per_inference` is the COUNTED one rather than a number that multiplies
                    // its own fork-choice weight. `verify_class_admission_v2` performs all four,
                    // and it needs the shape profile and the canonical job to do it.
                    //
                    // Neither used to ride the object, so the honest answer was no. Both ride it
                    // now, so the honest answer is "let the gate decide" — and the refusal that
                    // stood here was never a policy, it was the absence of a check. Genesis
                    // registrations do not reach this path at all; they are checked by
                    // `verify_palw_genesis_v2` against the catalog the ruleset id commits to.
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err(format!("class {class_id} registered on a network with no V2 bundle"));
                    };
                    let Some(carriage) = admission.as_ref() else {
                        return Err(format!(
                            "class {class_id} carries no shape profile, so coverage, ladder depth, court cost and pwu                              cannot be checked (ADR-0049 Decision H)"
                        ));
                    };
                    // Decision H's share rule: an entrant joins at the MINIMUM grantable share and
                    // no more. A registrant that could name its own permille would be donating
                    // itself an arbitrary slice of every incumbent's cadence, which is the share
                    // table's own conservation rule read backwards.
                    // **The entrant's difficulty is the chain's, not the registrant's** (audit
                    // M2-12). `initial_target` seeds the class's own retarget, so a registrant
                    // naming a huge one mines its class for free until the first retarget catches
                    // up — and naming a tiny one makes the class unminable, which is a way to park
                    // a share nobody can use. The base class's live target is what a registration
                    // is offered (`palw_v2_registration_terms`), and it is what must arrive.
                    self.palw_v2_class_registration_starts_at_the_chains_target(state, object)?;
                    // **An entrant joins at the minimum grantable share — or at NOTHING, when no
                    // certified family can prosecute it** (ADR-0069 Decisions 5 and 6).
                    //
                    // Decision H's rule was `share == min_grantable`, full stop, and
                    // `min_grantable` is never zero (`⌈10⁶/(tol·E)⌉.max(1)`). ADR-0069 then refused
                    // a nonzero grant to a family no drill has certified. Together those two closed
                    // the door completely: measured on this build, the adjudicator catalogs 44
                    // kernels and the certified families cover 37, so a class reaching one of the
                    // other 7 was statically adjudicable, refused weight by the gate, and refused
                    // registration by this line — it could not join at ALL.
                    //
                    // That is the opposite of what ADR-0069 decided. Weight is what certification
                    // buys; EXISTENCE is not, and Decision 5 made a zero grant expressible in
                    // `granted_share_table_v2` precisely so an uncertified family could register,
                    // produce, gossip and count for liveness while its adjudication was built. This
                    // path never let a zero through, so that state was reachable only at genesis —
                    // which is how the model tiers sat at 0‰ while nobody else could.
                    //
                    // The share is still exactly one value, so a registrant cannot choose it — the
                    // thing Decision H protects. What changed is that the value is a function of
                    // the class's own graph rather than a constant.
                    //
                    // **The certified set is the NETWORK's, not this process's.** The drilled
                    // registry is filled by a crate consensus links only as a dev dependency
                    // (ADR-0042 Decision 4: "a node's consensus never links a model runtime"), so
                    // reading it here would make this rule correct on a producing node and wrong on
                    // a validating one — two nodes disagreeing about a block, which is the failure
                    // `court_e2e_root` exists to prevent. `palw_rc_certified_families_v1` derives
                    // the same set from profiles every node already has, and hashes to that root.
                    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
                    // ADR-0075 Decision 4: the chain's own certified families count exactly as
                    // the genesis ones — an entrant whose family a `FamilyCertified` object
                    // certified joins at the floor, not weightless.
                    let chain_certified =
                        state.chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt);
                    let prosecutable = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
                        bundle.court_e2e_root,
                        &certified,
                        &chain_certified,
                        &kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&carriage.profile),
                    )
                    .map_err(|e| format!("class {class_id}: {e}"))?
                    .is_some();
                    let floor = state_params.min_grantable_share_permille();
                    // **ADR-0145 §7 and I4, past `palw_admission_independence`: registration buys
                    // existence, and cadence is earned.** The minimum grantable share is not a
                    // small number — it is a permille taken from every incumbent by donation, and
                    // it moves their weight, their budget and (through
                    // `attempt_target_seed_v1(share, pwu)`) their difficulty, for a class nobody
                    // has yet verified. Past the fence every entrant joins weightless, exactly as
                    // an uncertified one already does, and earns its share from the registry once
                    // it has passed an admission its own registrant cannot grant it.
                    //
                    // This gate and the transition must agree or no prosecutable class could
                    // register at all: the fold refuses a bought registration carrying any share
                    // (`RegistrationTakesNoShare`), so a `required` of `floor` here would be a rule
                    // demanding exactly what the next layer rejects.
                    let independence = self.palw_admission_independence_at(point.daa_score);
                    let required = if prosecutable && !independence { floor } else { 0 };
                    if *share_permille != required {
                        return Err(if prosecutable && !independence {
                            format!(
                                "class {class_id} registers at {share_permille}‰; a post-genesis entrant joins at the \
                                 minimum grantable share ({floor}‰) — ADR-0049 Decision H"
                            )
                        } else if independence {
                            format!(
                                "class {class_id} registers at {share_permille}‰; past palw_admission_independence a \
                                 registration buys existence and not cadence, so every entrant joins at 0‰ and earns its \
                                 share from an admission it has passed — ADR-0145 §7"
                            )
                        } else {
                            format!(
                                "class {class_id} registers at {share_permille}‰; no end-to-end certified family covers \
                                 the kernels it reaches, so it joins WEIGHTLESS (0‰) and earns cadence once some build \
                                 certifies a backend for it — ADR-0069 Decision 6"
                            )
                        });
                    }
                    // **And WHO is registering it** (launch blockers §3). Everything above is a
                    // fact about the graph and the share; none of it is a fact about the sender.
                    // A registration takes a permille from EVERY incumbent through
                    // largest-remainder donation, and the share's own doc says "whoever may
                    // register a class may fund it, and nobody else may move a permille" — there
                    // was no `whoever`. Any stranger could move the cadence table for a fee.
                    //
                    // The registrant must hold an ACTIVE bond and have signed the class it is
                    // registering together with the share it is taking. Not a permission system:
                    // the smallest answer to "who", denominated in the collateral every other
                    // authority on this chain is denominated in.
                    self.palw_v2_class_registration_is_signed(state, object)?;
                    // **The certified family set, from this build** (ADR-0069 Decision 5). The
                    // gate reads it only for a registration asking a nonzero share, and it refuses
                    // a set that does not hash to `bundle.court_e2e_root` — so a node whose court
                    // can play a different set of families than the network agreed to is stopped
                    // here rather than quietly granting or refusing weight on its own authority.
                    // The set is filled by the node's boot drill; the boot gate is what makes sure
                    // it agrees with the network's pin before a block is ever validated.
                    // **The gate is called with the fences this block runs under** (audit D H-3).
                    //
                    // This called `verify_class_admission_v3` — six arguments, `ladder: None` and
                    // `court: None` — which is the gate as it stood before ADR-0077 Phase B and
                    // ADR-0082 existed. Two consequences, on the ONLY permissionless registration
                    // path there is: every graph-v5 profile was refused
                    // `FusedAttentionNeedsTheKaryCourt` even on a chain whose `palw_kary_court` is
                    // armed (ADR-0082's own U-08 route, closed), and the close-chunk, ladder and
                    // `CourtWindowTooShort` bounds were never evaluated against the armed court at
                    // all. Every one of the three reads a fence, and a fence resolved anywhere but
                    // at the ONE accessor is a rule two nodes can answer differently — so all
                    // three come from the accessors above, at this block's own DAA score.
                    //
                    // `decode_rules` is `false` and not a fence read: ADR-0082 Decision 10's
                    // numerator is not in the transition, and `Params::validate_palw_v2` refuses to
                    // start a node that armed `palw_fp_decode_rules` (audit D M-1), so there is no
                    // configuration in which this may be anything else.
                    let court = match self.palw_kary_court_active_at(point.daa_score) {
                        false => None,
                        true => {
                            let derived = self
                                .palw_court_params_at(point.daa_score)
                                .ok_or_else(|| format!("class {class_id} is registered on a chain with no V2 ruleset"))?
                                .map_err(|e| format!("class {class_id} is judged under a court with no shape: {e}"))?;
                            Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                dissection_arity: derived.dissection_arity(),
                                prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                window_court_daa: kaspa_consensus_core::palw_class_admission_v2::palw_class_court_window_for_shape_v1(
                                    bundle.state.window_court(),
                                    self.palw_model_court_window_active_at(point.daa_score),
                                    self.palw_held_context_at(point.daa_score),
                                    &derived,
                                    &carriage.profile,
                                )
                                .map_err(|e| format!("class {class_id}: {e}"))?,
                            })
                        }
                    };
                    let ladder = self.palw_context_ladder_at(point.daa_score).then(|| {
                        kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
                            &carriage.profile,
                            court,
                            bundle.court.max_step_leaf_count(),
                        )
                    });
                    // **RFC-0002 Phase H (`palw_tir_only_v1`): past it a new class is an IR program or an IR pipeline** — a legacy-family
                    // registration (an admission carriage of hand-written kernels) is refused by name, before the graph walk. Dormant
                    // everywhere: no preset arms it.
                    if self.palw_tir_only_at(point.daa_score) {
                        return Err(format!(
                            "class {class_id} is a legacy-family registration and palw_tir_only_v1 admits only IR programs and pipelines (RFC-0002 Phase H)"
                        ));
                    }
                    kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v9(
                        bundle,
                        &carriage.profile,
                        &carriage.canonical,
                        object,
                        // The same committed set the share rule above read — see its note on why
                        // consensus must not read the drilled registry — and the chain's own.
                        &certified,
                        &chain_certified,
                        ladder.flatten(),
                        court,
                        false,
                        // ADR-0102: the per-token lift kernel is admitted by its fence alone.
                        self.palw_token_lift_at(point.daa_score),
                        // ADR-0093 Decision 6: past its fence, a fused tile must be one head's.
                        self.palw_fused_dissectable_at(point.daa_score),
                        // **F4, on the economic bundle's fence.** Past it the ladder is compared
                        // against the deepest job the class can LEGALLY run rather than against a
                        // count of one decode call — so a class is refused at REGISTRATION instead
                        // of being admitted and then refused, silently, when it runs its own jobs.
                        // Resolved at the block, unlike the claim-borne half of the same fence: a
                        // registration is decided once and never re-derived.
                        self.palw_canonical_work_daa.is_some_and(|height| point.daa_score >= height),
                        // ADR-0103: the held map is admitted by its fence alone, and a held class's
                        // walls are read under the regime — the PanelDa fence says whether its
                        // widest job commits with no ids.
                        kaspa_consensus_core::palw_class_admission_v2::PalwHeldAdmissionV1 {
                            armed: self.palw_held_context_at(point.daa_score),
                            panel_da: self.palw_panel_da_at(point.daa_score),
                        },
                        self.palw_kimi_k3_at(point.daa_score),
                        // 2026-09-23 audit C-4: past its fence a non-fused class's priced geometry
                        // must fit the query row its graph reads.
                        self.palw_audit_2026_09_23_at(point.daa_score),
                        // P0a: a version-3 profile (the derived GDN key-head count) is admitted by its
                        // fence alone, and past it a V1/V2 hybrid whose head counts differ may not
                        // register a map written over one head count.
                        self.palw_gdn_key_heads_at(point.daa_score),
                    )
                    .map_err(|e| format!("class {class_id} is not admissible: {e}"))?;
                    // ADR-0152 §4-ter C5: past `palw_offence_attribution` a held class is admitted
                    // only where its attention lie is attributable — its dissection answerable
                    // (`palw_held_class_unanswerable_v1`: the context bound, a recurrent layer, a
                    // compute turn past the cap — a pure function of the profile).
                    kaspa_consensus_core::palw_class_admission_v2::palw_held_class_is_attributable_v1(
                        &carriage.profile,
                        self.palw_offence_attribution_at(point.daa_score),
                    )
                    .map_err(|e| format!("class {class_id} is not admissible: {e}"))?;
                    // **ADR-0152 v3.1 addendum §4-bis.8: every claim of the class must be
                    // attributable.** Past `palw_offence_attribution` a registration is refused
                    // unless its canonical job is the formula's (a class too narrow for it is
                    // refused too, so `IdentityNotDerivable` is no registrant's way out of J5), its
                    // logits row is a provable step output (which refuses Float32), it reaches no
                    // Kimi K3 kernel, and a canonical prompt past J5b's inline bound is committed in
                    // the Merkle form. Processor only: the gate decides, the fold never re-derives a
                    // registration.
                    if self.palw_offence_attribution_at(point.daa_score) {
                        kaspa_consensus_core::palw_attempt_rules_v1::palw_attributable_class_v1(
                            &carriage.profile,
                            &carriage.canonical,
                            false,
                            self.palw_prompt_ids_form_at(point.daa_score),
                        )
                        .map_err(|e| format!("class {class_id} is not attributable past palw_offence_attribution: {e}"))?;
                    }
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
                // **Everything below carries no authorization this layer can check, and is
                // therefore refused on the transaction path** (audit M-01).
                //
                // Each of these folds into consensus state and each was reachable by any stranger
                // for one ordinary transaction fee:
                //
                // * `BondRetireRequested { bond }` names a PUBLIC premine outpoint and flips that
                //   bond to `Retiring`. There is no inverse and no owner binding, so one
                //   transaction permanently stops any producer — including, on the RC, the only
                //   one. Re-admitting it needs an owner signature over the bond key.
                // * `ClassFrozen` carries a contradiction certificate whose signatures
                //   `check_class_contradiction_shape_v2` explicitly defers to "the acceptance
                //   layer" — which had no arm for it. `adjudicate_class_contradiction_v1`, the
                //   version that takes a verifier, is wired only into the other band. A forged
                //   certificate freezes a class permanently (there is deliberately no
                //   `ClassUnfrozen`); inert today only because the liveness floor is exempt, and
                //   armed the moment a second class exists.
                // * `PanelBound` is refused here, for the reason the ride list states.
                //   `BondRegistered` is NOT: the ride list admits it once its carrier locks the
                //   collateral it declares, and the arm below checks the one thing the carrier
                //   cannot — that the registrant holds the key it is registering.
                //
                // Refusing is not the final answer for either — a bond must eventually be able to
                // retire and an emergency freeze must eventually be pullable — but a door that
                // cannot be authenticated is better shut than open, and `palw_lifecycle_objects_v2`
                // refuses them at admission for the same reason. This arm is the second lock.
                // **A retirement is authorised by the bond's own key, and by nothing else.**
                //
                // This used to be a blanket refusal, which was correct about the danger and wrong
                // as a resting state: retirement is the only writer of `Retiring`, an `Active`
                // bond's collateral is unconditionally locked, and the C-08 burn is collected only
                // from a bond the lock has released. Refusing it forever means the collateral can
                // never be withdrawn and the slashed sompi never actually burns.
                //
                // So the door opens with the lock the refusal described: the same registrant-bond
                // signature check `ClassRegistered` performs a few arms above, over
                // `palw_bond_retirement_message_v2`, verified against the pubkey the bond itself
                // registered. A bond key is a public premine outpoint; the signature is what makes
                // naming one different from owning it.
                Obj::BondRetireRequested { bond, signature } => {
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a retirement names bond {bond:?} this chain does not have"))?;
                    if matches!(record.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }) {
                        return Err(format!("bond {bond:?} is already retiring"));
                    }
                    let message = kaspa_consensus_core::palw_state_v2::palw_bond_retirement_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        bond,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s retirement is not signed by the key it registered"));
                    }
                }
                Obj::ClassFrozen { .. } => {
                    return Err("a class freeze carries no verified contradiction; the object is refused".to_string());
                }
                // Objects the ride list already refuses, and objects with nothing to check here.
                // **This match is EXHAUSTIVE on purpose**: the `_ => {}` it replaces is what let
                // four money-moving object kinds through in silence, and an exhaustive match makes
                // adding a fifth a decision somebody has to write down.
                // **A registration is authorised by the key it declares.**
                //
                // This arm was a silent pass-through, and the only thing refusing an
                // unauthenticated registration was the ride list — one lock where the comment
                // three arms up says there should be two. The carrier proves the collateral output
                // exists, holds what is claimed and pays to the declared payee
                // (`palw_bond_registration_binds_its_carrier_v2`, in the extractor, where the
                // transaction is). What it cannot prove is that the registrant holds the key it
                // names, because anyone can pay to somebody else's script — and a registry is a
                // list of who may be SEATED, so padding it with keys nobody controls is not a
                // harmless gift.
                Obj::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload, capable_classes, signature } => {
                    // **The form the registrant signed, not the one the chain now holds.** A
                    // carried registration names its output by index with a zero transaction id,
                    // because the carrier's id is a function of the payload the signature goes
                    // into; the extractor substituted the real id on the way in. Verifying against
                    // the substituted key would reject every honest registration.
                    let signed_bond = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_bond_registration_signed_key_v2(bond);
                    let message = kaspa_consensus_core::palw_state_v2::palw_bond_registration_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &signed_bond,
                        pubkey,
                        operator_pubkey,
                        *collateral,
                        payout_payload,
                        capable_classes,
                    );
                    // **Past the fence the registration carries TWO signatures, and the second
                    // proves the operator identity is held rather than declared** (audit
                    // 2026-09-19).
                    //
                    // `operator_id` is the unit of panel dedup and of the executor exclusion, and
                    // it is derived from `operator_pubkey` — which this message signs under the
                    // BOND key. That proves the registrant chose those bytes; it proves nothing
                    // about holding them. Undeclared, a registrant could name a victim's identity
                    // and thereby take that victim out of the jury of every claim he produces.
                    //
                    // An ML-DSA-87 signature is a fixed `MLDSA87_SIG_LEN`, so a registration past
                    // the fence is exactly two of them and the split is unambiguous. That is why
                    // this needs no new object and no new state: the field already exists, and the
                    // fence is what says how to read it.
                    let operator_proof = if self.palw_operator_id_unique_at(point.daa_score) {
                        let sig_len = kaspa_txscript::MLDSA87_SIG_LEN;
                        if signature.len() != 2 * sig_len {
                            return Err(format!(
                                "bond {bond:?}'s registration carries {} signature bytes; past the operator-possession fence it \
                                 carries two signatures ({} bytes): the bond's, then the operator key's",
                                signature.len(),
                                2 * sig_len
                            ));
                        }
                        Some(signature.split_at(sig_len))
                    } else {
                        None
                    };
                    let bond_sig = operator_proof.map(|(first, _)| first).unwrap_or(signature.as_slice());
                    if !Self::verify_mldsa87_with_context_bool(
                        pubkey,
                        message.as_byte_slice(),
                        bond_sig,
                        kaspa_consensus_core::palw_state_v2::PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s registration is not signed by the key it declares"));
                    }
                    if let Some((_, operator_sig)) = operator_proof {
                        let proof = kaspa_consensus_core::palw_state_v2::palw_operator_possession_message_v1(
                            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                                self.network_id_bytes.as_slice(),
                                Some(self.genesis.hash),
                            ),
                            &signed_bond,
                            pubkey,
                            operator_pubkey,
                        );
                        if !Self::verify_mldsa87_with_context_bool(
                            operator_pubkey,
                            proof.as_byte_slice(),
                            operator_sig,
                            kaspa_consensus_core::palw_state_v2::PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT,
                        ) {
                            return Err(format!(
                                "bond {bond:?} declares an operator identity it does not prove it holds: the second signature does \
                                 not verify under the operator key"
                            ));
                        }
                    }
                }
                // **A capability declaration is authorised by the bond's own key, and by nothing
                // else** (ADR-0071 Decision 3) — the same lock retirement carries, for the same
                // reason: a bond key is a public outpoint, so naming one must differ from owning
                // it. Volunteering somebody else's collateral for duty is not a harmless gift when
                // the duty accounting convicts the seats the draw names.
                Obj::BondCapabilityDeclared { bond, capable_classes, signature } => {
                    let record = state
                        .bond(bond)
                        .ok_or_else(|| format!("a capability declaration names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_state_v2::palw_bond_capability_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        bond,
                        capable_classes,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_BOND_CAPABILITY_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s capability declaration is not signed by the key it registered"));
                    }
                }
                // **ADR-0062 SA-1: an accusation is authorised by the accuser's own bond key, and
                // by nothing else** — the same lock a retirement carries, for the same reason. A
                // bond key is a public outpoint, so naming one must differ from owning it:
                // unsigned, anyone could put an honest producer's claim under a session that
                // pauses its path to `Final`, under a stranger's identity.
                //
                // The FENCE is checked here as well as in the fold. The fold refusing it is what
                // makes it a rule; this refusing it is what stops a block from being folded at all
                // on a network where the rule is dormant.
                Obj::SeatReadinessProvedV2 { bond, class_id, span, proof, signature } => {
                    if !self.palw_model_registry_at(point.daa_score) {
                        return Err(format!("a possession proof for class {class_id} below the model registry's fence"));
                    }
                    if !self.palw_readiness_v2_at(point.daa_score) {
                        return Err(format!("a V2 possession proof for class {class_id} below readiness V2's fence (ADR-0133 §11.2)"));
                    }
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a possession proof names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_model_registry_v1::palw_seat_readiness_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &borsh::to_vec(bond).expect("a bond key is borsh-serializable"),
                        class_id,
                        *span,
                        proof,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_model_registry_v1::PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s possession proof is not signed by the key it registered"));
                    }
                }
                Obj::SeatReadinessProved { bond, class_id, span, opening, signature } => {
                    // ADR-0135: below the fence the object does not exist; above it the seat's own
                    // key must sign the proof, or a relayer could volunteer another bond's collateral.
                    if !self.palw_model_registry_at(point.daa_score) {
                        return Err(format!("a readiness proof for class {class_id} below the model registry's fence"));
                    }
                    if self.palw_readiness_v2_at(point.daa_score) {
                        return Err(format!(
                            "class {class_id}: the one-leaf possession proof is superseded at this height (ADR-0133 §11.2)"
                        ));
                    }
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a readiness proof names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_model_registry_v1::palw_seat_readiness_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &borsh::to_vec(bond).expect("a bond key is borsh-serializable"),
                        class_id,
                        *span,
                        opening.leaf_index,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_model_registry_v1::PALW_SEAT_READINESS_V1_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s readiness proof is not signed by the key it registered"));
                    }
                }
                Obj::ClassManifestV2 { class_id, artifact_bytes, registrant_bond, signature } => {
                    if !self.palw_model_registry_at(point.daa_score) {
                        return Err(format!("a manifest for class {class_id} below the model registry's fence"));
                    }
                    let record = state
                        .bond(registrant_bond)
                        .ok_or_else(|| format!("a manifest names bond {registrant_bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_model_registry_v1::palw_class_manifest_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &borsh::to_vec(registrant_bond).expect("a bond key is borsh-serializable"),
                        class_id,
                        *artifact_bytes,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_model_registry_v1::PALW_CLASS_MANIFEST_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {registrant_bond:?}'s manifest is not signed by the key it registered"));
                    }
                }
                Obj::ObjectiveOffence { kind, accused, evidence_id, evidence } => {
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
                                let finding = kaspa_consensus_core::palw_offence_attribution_v1::palw_check_panel_false_valid_at_v2(
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
                                continue;
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
                                let finding = kaspa_consensus_core::palw_offence_attribution_v1::palw_check_executor_refuted_at_v1(
                                    state,
                                    accused,
                                    evidence,
                                    state_params.fp_decode_rules_at(point.daa_score),
                                    false,
                                    self.palw_identity_rules_v1(point.daa_score),
                                    self.palw_session_rule_at(point.daa_score),
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
                                continue;
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
                                continue;
                            }
                            PalwOffenceKindV1::ExecutorEquivocation | PalwOffenceKindV1::CourtExecutorGuilty => {}
                            // Refused above; listed so a routing change that forgets them fails to compile.
                            PalwOffenceKindV1::DaDefault | PalwOffenceKindV1::CourtConviction => {}
                        }
                    } else if matches!(
                        kind,
                        kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2
                            | kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted
                            | kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::TirIdentityMismatch
                    ) {
                        return Err(kaspa_consensus_core::palw_offence_v1::PalwOffenceVerifyError::AttributionDormant.to_string());
                    }
                    kaspa_consensus_core::palw_offence_v1::palw_verify_objective_offence_v1(
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
                        kaspa_consensus_core::palw_offence_v1::palw_panel_contradiction_convicts_execution_v1(
                            &payload.contradiction,
                            execution_root,
                            artifact_root,
                            ladder,
                        )
                        .map_err(|e| e.to_string())?;
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
                Obj::DefaultAccused { claim, missing_event_index, accuser, signature } => {
                    if !self.palw_da_court_at(point.daa_score) {
                        return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                    }
                    let record =
                        state.bond(accuser).ok_or_else(|| format!("an accusation names bond {accuser:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_state_v2::palw_da_accusation_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        claim,
                        *missing_event_index,
                        accuser,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("claim {claim}'s accusation is not signed by the bond it names"));
                    }
                }
                // **ADR-0099 Decision 5, built by ADR-0100: the one-move court's accusation.** Two
                // gates in the order every court move uses — the FENCE, then the accuser's bond
                // key over the session id — then the object's shape, the ruleset's own close
                // ceiling, the claim it names, and the verdict DERIVED here at the ladder the fold
                // will derive it at too (the extras carry that ladder to the fold). A fused site is
                // refused, because its verdict convicts nobody; an accusation whose refutation does
                // not adjudicate is refused too — P0-8's rule on both sides. The FENCE is checked
                // here as well as in the fold, for the DA court's reason.
                // **RFC-0002 Phase F (tag 62): an IR claim accused in one move.** Signed over its
                // session id by the accuser's registered key; about a live claim of an IR class,
                // its executor and roots the claim's; and the verdict it DECLARES is the one its proof
                // produces under this block's court (`palw_tir_one_move_verdict_v1`, the court close's
                // own adjudication) — refused in either direction otherwise, so the fold may apply it.
                Obj::TirShardCourtAccused { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_tir_at(point.daa_score) {
                        return Err(format!("claim {claim_id}: an IR accusation is refused: palw_tir_v1 is not in force (RFC-0002)"));
                    }
                    // RFC-0004 A6: an evaluation close rides this accusation past `palw_improvement_v1` only.
                    if accusation.proof.is_eval_v1() && !self.palw_improvement_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: an evaluation accusation is refused: palw_improvement_v1 is not in force (RFC-0004)"
                        ));
                    }
                    kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_shape_v1(accusation)
                        .map_err(|e| format!("claim {claim_id}: {e}"))?;
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "an IR one-move accusation on a network with no V2 court parameters".to_string())?;
                    let record = state
                        .bond(&accusation.accuser_bond)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let session_id = kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_session_id_v1(
                        domain.as_byte_slice(),
                        accusation,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        session_id.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_tir_one_move_v1::PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1,
                    ) {
                        return Err(format!("claim {claim_id}'s IR accusation is not signed by the bond it names"));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    if state.tir_class_v1(&claim.class_id).is_none() {
                        return Err(format!("claim {claim_id} is not a claim of an IR class"));
                    }
                    if claim.bond != accusation.executor_bond
                        || claim.execution_root != accusation.execution_root
                        || claim.trace_root != accusation.trace_root
                    {
                        return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                    }
                    let ladder = state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                    // RFC-0002 F7: under the held regime a cone accusation at a dissected leaf opens a
                    // dissection there (the fold asks the same question); it declares the conviction
                    // it is prosecuting. Every other accusation's verdict is re-derived and compared.
                    let outcome = kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_outcome_v1(
                        state,
                        claim,
                        accusation,
                        court,
                        ladder,
                        self.palw_prompt_ids_form_at(point.daa_score),
                        self.palw_held_context_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {claim_id}: the IR accusation does not adjudicate: {e}"))?;
                    match outcome {
                        kaspa_consensus_core::palw_tir_one_move_v1::PalwTirOneMoveOutcomeV1::Verdict(derived) => {
                            if derived != accusation.verdict {
                                return Err(format!(
                                    "claim {claim_id}: the IR accusation declares {:?} and its proof produces {derived:?}",
                                    accusation.verdict
                                ));
                            }
                        }
                        kaspa_consensus_core::palw_tir_one_move_v1::PalwTirOneMoveOutcomeV1::NeedsDissection { leaf } => {
                            if accusation.verdict != kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty {
                                return Err(format!(
                                    "claim {claim_id}: an IR accusation at dissected leaf {leaf} opens a dissection and declares \
                                     ExecutorGuilty, not {:?}",
                                    accusation.verdict
                                ));
                            }
                        }
                    }
                }
                // **RFC-0003 §I.4.7 (tensor claims' court under the held regime): a pipeline claim accused in one
                // move.** The IR accusation's gates with the generative registry: signed over its session id by
                // the accuser's registered key; about a live claim of a generative class, its executor and roots the
                // claim's; and the verdict it DECLARES is the one its proof produces under this block's court
                // (`palw_gen_one_move_verdict_v1`, the court close's own adjudication) — refused in either direction
                // otherwise, so the fold may apply it. A cone accusation at a dissected leaf declares ExecutorGuilty
                // and opens a dissection under the held regime.
                Obj::GenShardCourtAccused { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_gen_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: a generative accusation is refused: palw_gen_v1 is not in force (RFC-0003)"
                        ));
                    }
                    kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_shape_v1(accusation)
                        .map_err(|e| format!("claim {claim_id}: {e}"))?;
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a generative one-move accusation on a network with no V2 court parameters".to_string())?;
                    let record = state
                        .bond(&accusation.accuser_bond)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let session_id = kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_session_id_v1(
                        domain.as_byte_slice(),
                        accusation,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        session_id.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_gen_one_move_v1::PALW_GEN_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1,
                    ) {
                        return Err(format!("claim {claim_id}'s generative accusation is not signed by the bond it names"));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    if state.gen_class_v1(&claim.class_id).is_none() {
                        return Err(format!("claim {claim_id} is not a claim of a pipeline class"));
                    }
                    if claim.bond != accusation.executor_bond
                        || claim.execution_root != accusation.execution_root
                        || claim.trace_root != accusation.trace_root
                    {
                        return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                    }
                    let ladder = state.class_step_ladder_v1(&claim.class_id, self.palw_court_step_ladder_at(point.daa_score, court));
                    let outcome = kaspa_consensus_core::palw_gen_one_move_v1::palw_gen_one_move_outcome_v1(
                        state,
                        claim,
                        accusation,
                        court,
                        ladder,
                        self.palw_prompt_ids_form_at(point.daa_score),
                        self.palw_held_context_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {claim_id}: the generative accusation does not adjudicate: {e}"))?;
                    match outcome {
                        kaspa_consensus_core::palw_gen_one_move_v1::PalwGenOneMoveOutcomeV1::Verdict(derived) => {
                            if derived != accusation.verdict {
                                return Err(format!(
                                    "claim {claim_id}: the generative accusation declares {:?} and its proof produces {derived:?}",
                                    accusation.verdict
                                ));
                            }
                        }
                        kaspa_consensus_core::palw_gen_one_move_v1::PalwGenOneMoveOutcomeV1::NeedsDissection { leaf } => {
                            if accusation.verdict != kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty {
                                return Err(format!(
                                    "claim {claim_id}: a generative accusation at dissected leaf {leaf} opens a dissection and                                      declares ExecutorGuilty, not {:?}",
                                    accusation.verdict
                                ));
                            }
                        }
                    }
                }
                // **RFC-0003 decision 22 (the held leaf challenge): a named-leaf challenge with the declaration of its
                // close.** Signed over its digest by the accuser's registered key, within the ruleset's own carriage
                // count; everything about the claim (live, its class, its roots, the leaf, the open sessions, the
                // accuser's standing) is the fold's, where the state is in hand, and the rehearsal drops what the fold
                // refuses. It adjudicates nothing — the close it pins is graded, once, by the completing chunk.
                Obj::HeldLeafChallengeDeclared { challenge } => {
                    let claim_id = challenge.claim;
                    if !self.palw_held_close_chunks_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: a held leaf challenge is refused: palw_held_close_chunks_v1 is not in force (RFC-0003)"
                        ));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a held leaf challenge on a network with no V2 court parameters".to_string())?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    kaspa_consensus_core::palw_held_close_v1::check_held_leaf_challenge_acceptance_v1(
                        state,
                        domain.as_byte_slice(),
                        challenge,
                        court,
                        |key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        },
                    )
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                }
                Obj::ShardCourtAccused { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_shard_court_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: the one-move court is not armed on this network (ADR-0099 Decision 5)"
                        ));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a one-move accusation on a network with no V2 court parameters".to_string())?;
                    let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                    let record = state
                        .bond(&accusation.accuser_bond)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let session_id =
                        kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_session_id_v1(domain.as_byte_slice(), accusation);
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        session_id.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s accusation is not signed by the bond it names"));
                    }
                    // ADR-0119 Decision 4: the shape is bounded at the CLAIM's ladder, so the claim
                    // is found first — a held class's recorded ladder, every other the network's.
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    let ladder = state.class_step_ladder_v1(&claim.class_id, ladder);
                    accusation.validate_shape(ladder).map_err(|e| format!("claim {claim_id}: {e}"))?;
                    let bytes = kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(accusation);
                    if bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim_id}: the accusation carries {bytes} bytes and this ruleset prices a close at {}",
                            court.max_close_bytes()
                        ));
                    }
                    if claim.bond != accusation.executor_bond
                        || claim.execution_root != accusation.execution_root
                        || claim.trace_root != accusation.trace_root
                    {
                        return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                    }
                    let class = state
                        .class(&claim.class_id)
                        .ok_or_else(|| format!("claim {claim_id} names class {} this chain does not have", claim.class_id))?;
                    // t12 (`palw_audit_2026_09_23`): bound to the claim before a fused site is deferred —
                    // the same function, at the same fence, the fold derives the verdict with.
                    let bound_to = kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2 {
                        execution_root: claim.execution_root,
                        class_id: claim.class_id,
                        artifact_root: class.artifact_root,
                    };
                    match kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_verdict_at_v2(
                        accusation,
                        &bound_to,
                        ladder,
                        self.palw_audit_2026_09_23_at(point.daa_score),
                    ) {
                        // ADR-0103 Decision 5: under the held regime the accusation at a fused leaf IS
                        // the challenge — the fold opens the dissection there. Dormant, refused.
                        Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection)
                            if !self.palw_held_context_at(point.daa_score) =>
                        {
                            return Err(format!(
                                "claim {claim_id}: leaf {} is a fused-attention site, which one move cannot try, and this network \
                                 opens no held dissection at a named leaf",
                                accusation.leaf_index
                            ));
                        }
                        // ADR-0152 §4-ter (A-held): past `palw_offence_attribution` a held class no
                        // honest party can dissect inside a turn (the 2M row) opens no session — the
                        // fold refuses it (`ShardCourtHeldSiteUnanswerable`) off the same mirror.
                        Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection)
                            if self.palw_offence_attribution_at(point.daa_score)
                                && state_params.held_class_is_unanswerable_v1(&claim.class_id) =>
                        {
                            return Err(format!(
                                "claim {claim_id}: leaf {} is a fused-attention site of held class {}, whose dissection no honest \
                                 party can play inside a turn: refused, not opened (ADR-0152 §4-ter)",
                                accusation.leaf_index, claim.class_id
                            ));
                        }
                        Ok(_) => {}
                        Err(e) => return Err(format!("claim {claim_id}: the accusation does not adjudicate: {e}")),
                    }
                }
                // **ADR-0103 Decision 1: the checkpoint court.** The shard court's gates in its order —
                // the fence, the accuser's key over the session id, the ceiling, the claim — and the
                // verdict DERIVED here at the ladder the fold derives it at.
                Obj::CheckpointAccused { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_held_context_at(point.daa_score) {
                        return Err(format!("claim {claim_id}: the held regime is not armed on this network (ADR-0103)"));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a checkpoint accusation on a network with no V2 court parameters".to_string())?;
                    let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                    let record = state
                        .bond(&accusation.accuser_bond)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser_bond))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let session_id = kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_session_id_v1(
                        domain.as_byte_slice(),
                        accusation,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        session_id.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_checkpoint_court_v1::PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s checkpoint accusation is not signed by the bond it names"));
                    }
                    let bytes = kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_accusation_bytes_v1(accusation);
                    if bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim_id}: the checkpoint accusation carries {bytes} bytes and this ruleset prices a close at {}",
                            court.max_close_bytes()
                        ));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    if claim.bond != accusation.executor_bond
                        || claim.execution_root != accusation.execution_root
                        || claim.trace_root != accusation.trace_root
                    {
                        return Err(format!("claim {claim_id}: the accusation's executor or roots are not the claim's"));
                    }
                    kaspa_consensus_core::palw_checkpoint_court_v1::palw_checkpoint_court_verdict_v1(
                        accusation,
                        claim.class_id,
                        // ADR-0119 Decision 4: the CLAIM's ladder.
                        state.class_step_ladder_v1(&claim.class_id, ladder),
                    )
                    .map_err(|e| format!("claim {claim_id}: the checkpoint accusation does not adjudicate: {e}"))?;
                }
                // **ADR-0103 Decision 4: the held DA court's two moves.** The fence, the DA court's
                // own fence, the signer (the accuser's key; the claim's producer for an answer), the
                // ceiling, and the unit bounded — or answered — against the claim's own roots.
                // **The second IR fence: an IR step demand** (evidence transport C), keyed by the claim
                // alone. The fence, the DA court, a step unit inside the widest execution, the ruleset's
                // close ceiling and the accuser's registered key over the demand; the claim and its
                // class are the fold's (`open_da_session_tir_step_v1`), and a unit past the claim's own
                // execution is the accused's to prove (`TirStepOutOfRange`).
                Obj::DefaultAccusedTirStep { accusation } => {
                    let claim = accusation.claim;
                    if !self.palw_tir_fence2_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: an IR step demand is refused: palw_tir_fence2 is not in force (RFC-0002)"
                        ));
                    }
                    if !self.palw_da_court_at(point.daa_score) {
                        return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "an IR step demand on a network with no V2 court parameters".to_string())?;
                    // The widest execution at the court's step ladder where the held regime rides it,
                    // the release's 2^22 otherwise; the fold holds the unit to the claim's class's.
                    let ladder = if self.palw_held_context_at(point.daa_score) {
                        self.palw_court_step_ladder_at(point.daa_score, court)
                            .max(kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES)
                    } else {
                        kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES
                    };
                    kaspa_consensus_core::palw_da_rcore_v1::palw_tir_step_unit_is_admissible_v1(&accusation.unit, ladder)
                        .map_err(|why| format!("claim {claim}: an IR step demand for {:?} is refused: {why}", accusation.unit))?;
                    let bytes = borsh::to_vec(accusation.as_ref()).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                    if bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim}'s IR step demand is {bytes} bytes, above this ruleset's {}-byte close ceiling",
                            court.max_close_bytes()
                        ));
                    }
                    let record = state
                        .bond(&accusation.accuser)
                        .ok_or_else(|| format!("an IR step demand names bond {:?} this chain does not have", accusation.accuser))?;
                    let message = kaspa_consensus_core::palw_da_rcore_v1::palw_tir_step_accusation_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &claim,
                        &accusation.unit,
                        &accusation.accuser,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_da_rcore_v1::PALW_TIR_STEP_ACCUSATION_MLDSA87_CONTEXT_V1,
                    ) {
                        return Err(format!("claim {claim}'s IR step demand is not signed by the bond it names"));
                    }
                }
                // **Pipeline-claim data availability: a pipeline step demand** (spec 17 §17.14, object tag 83),
                // keyed by the claim alone. The fence (`palw_improvement_v1`), the DA court, a unit inside the
                // widest execution, the ruleset's close ceiling and the accuser's registered key over the
                // demand; the claim and its kind are the fold's (`open_da_session_pipeline_step_v1`), and a unit
                // past the claim's own stage is the accused's to prove (`PipelineStepOutOfRange`).
                Obj::DefaultAccusedPipelineStep { accusation } => {
                    let claim = accusation.claim;
                    if !self.palw_pipeline_da_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim}: a pipeline step demand is refused: palw_improvement_v1 is not in force (RFC-0004 Phase F)"
                        ));
                    }
                    if !self.palw_da_court_at(point.daa_score) {
                        return Err(format!("claim {claim}: the data-availability court is not armed on this network (ADR-0062)"));
                    }
                    let court = self
                        .palw_court_params_v2
                        .as_ref()
                        .ok_or_else(|| "a pipeline step demand on a network with no V2 court parameters".to_string())?;
                    kaspa_consensus_core::palw_pipeline_da_v1::palw_pipeline_step_unit_is_admissible_v1(&accusation.unit)
                        .map_err(|why| format!("claim {claim}: a pipeline step demand for {:?} is refused: {why}", accusation.unit))?;
                    let bytes = borsh::to_vec(accusation.as_ref()).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                    if bytes > court.max_close_bytes() {
                        return Err(format!(
                            "claim {claim}'s pipeline step demand is {bytes} bytes, above this ruleset's {}-byte close ceiling",
                            court.max_close_bytes()
                        ));
                    }
                    let record = state.bond(&accusation.accuser).ok_or_else(|| {
                        format!("a pipeline step demand names bond {:?} this chain does not have", accusation.accuser)
                    })?;
                    let message = kaspa_consensus_core::palw_pipeline_da_v1::palw_pipeline_step_accusation_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &claim,
                        &accusation.unit,
                        &accusation.accuser,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_pipeline_da_v1::PALW_PIPELINE_ACCUSATION_MLDSA87_CONTEXT_V1,
                    ) {
                        return Err(format!("claim {claim}'s pipeline step demand is not signed by the bond it names"));
                    }
                }
                Obj::DefaultAccusedHeld { accusation } => {
                    let claim_id = accusation.claim;
                    if !self.palw_held_context_at(point.daa_score) || !self.palw_da_court_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: a held accusation needs the held regime and the data-availability court (ADR-0103)"
                        ));
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                    let bytes = kaspa_consensus_core::palw_held_da_v1::palw_held_da_bytes_v1(accusation.as_ref());
                    if bytes > court.max_close_bytes() {
                        return Err(format!("claim {claim_id}: the held accusation carries {bytes} bytes, above the close ceiling"));
                    }
                    let record = state
                        .bond(&accusation.accuser)
                        .ok_or_else(|| format!("an accusation names bond {:?} this chain does not have", accusation.accuser))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let message =
                        kaspa_consensus_core::palw_held_da_v1::palw_held_da_accusation_message_v1(domain.as_byte_slice(), accusation);
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        &accusation.signature,
                        kaspa_consensus_core::palw_held_da_v1::PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s held accusation is not signed by the bond it names"));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("an accusation names claim {claim_id} this chain does not have"))?;
                    kaspa_consensus_core::palw_held_da_v1::palw_held_da_check_accusation_v1(
                        &claim.execution_root,
                        &accusation.missing,
                        &accusation.binding,
                        self.palw_prompt_ids_form_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                    // **ADR-0111 Decision 3: a leaf is demanded only where the chain assigned it.**
                    // The demanding seat's own draw — keyed by the network domain, which is held here
                    // and not in the fold — must have put the leaf's interval in that seat's sample,
                    // so no seat can pick the leaf an executor must put on chain.
                    //
                    // **ADR-0152 DA-3 (M3): not past `palw_rcore_plus`**, where a named `StepLeaf` is
                    // free — any leaf inside the binding's bound, from a seat or not — and the per-seat
                    // session budget (DA-8) replaces the once-per-seat rule; drawn units, seeded by the
                    // accepting block, are what the accuser cannot choose.
                    if let kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf } = accusation.missing
                        && !state_params.rcore_plus_active_at(point.daa_score)
                    {
                        let panel =
                            state.panel(&claim_id).ok_or_else(|| format!("claim {claim_id} has no bound panel to demand from"))?;
                        let seat_index = panel
                            .seats
                            .iter()
                            .position(|seat| seat.bond == accusation.accuser)
                            .ok_or_else(|| format!("claim {claim_id}: a leaf's evidence is demanded by a seat of its panel"))?;
                        kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_demand_is_the_seats_v1(
                            &domain,
                            &panel.anchor,
                            &claim_id,
                            seat_index as u8,
                            &accusation.binding,
                            leaf,
                        )
                        .map_err(|e| format!("claim {claim_id}: {e}"))?;
                    }
                }
                Obj::MaterialDisclosedHeld { disclosure } => {
                    let claim_id = disclosure.claim;
                    if state_params.rcore_plus_active_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: MaterialDisclosedHeld is retired past palw_rcore_plus; MaterialDisclosedV2 answers (DA-1)"
                        ));
                    }
                    if !self.palw_held_context_at(point.daa_score) || !self.palw_da_court_at(point.daa_score) {
                        return Err(format!(
                            "claim {claim_id}: a held disclosure needs the held regime and the data-availability court (ADR-0103)"
                        ));
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("no court params")?;
                    let ladder = self.palw_court_step_ladder_at(point.daa_score, court);
                    let bytes = kaspa_consensus_core::palw_held_da_v1::palw_held_da_bytes_v1(disclosure.as_ref());
                    if bytes > court.max_close_bytes() {
                        return Err(format!("claim {claim_id}: the held disclosure carries {bytes} bytes, above the close ceiling"));
                    }
                    let claim = state
                        .claim(&claim_id)
                        .ok_or_else(|| format!("a disclosure names claim {claim_id} this chain does not have"))?;
                    let producer = state
                        .bond(&claim.bond)
                        .ok_or_else(|| format!("claim {claim_id}'s producing bond is not in this chain's registry"))?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    let message =
                        kaspa_consensus_core::palw_held_da_v1::palw_held_da_disclosure_message_v1(domain.as_byte_slice(), disclosure);
                    if !Self::verify_mldsa87_with_context_bool(
                        &producer.pubkey,
                        message.as_byte_slice(),
                        &disclosure.signature,
                        kaspa_consensus_core::palw_held_da_v1::PALW_HELD_DA_MLDSA87_DISCLOSE_CONTEXT,
                    ) {
                        return Err(format!("claim {claim_id}'s held disclosure is not signed by the bond that produced it"));
                    }
                    kaspa_consensus_core::palw_held_da_v1::palw_held_da_check_disclosure_v1(
                        &claim.execution_root,
                        &disclosure.missing,
                        &disclosure.binding,
                        &disclosure.disclosure,
                        // ADR-0119 Decision 4: the answer opens at the CLAIM's ladder, as the fold's.
                        state.class_step_ladder_v1(&claim.class_id, ladder),
                        self.palw_prompt_ids_form_at(point.daa_score),
                    )
                    .map_err(|e| format!("claim {claim_id}: {e}"))?;
                }
                // **ADR-0125 §7.3: a permit signed twice.** The lane open at this block, the named span
                // one the chain still keeps, its schedule granting the permit to the bond at the
                // span's width, both signatures under the bond's REGISTERED key (not a key the
                // evidence carries), and the permit not burned already.
                Obj::RoundPermitEquivocated { evidence } => {
                    use kaspa_consensus_core::palw_execution_lane_v1::{palw_execution_permit_of_v2, palw_execution_span_v1};
                    let lane = self
                        .palw_execution_lane_at(point.daa_score)
                        .ok_or_else(|| "round equivocation evidence where the execution lane is not open (ADR-0125)".to_string())?;
                    let span_daa = lane.schedule_span_daa_at(point.daa_score);
                    let span_now = palw_execution_span_v1(point.daa_score, span_daa);
                    if evidence.span > span_now || evidence.span + 1 < span_now {
                        return Err(format!(
                            "round equivocation names span {}, which the chain does not keep at span {span_now}",
                            evidence.span
                        ));
                    }
                    let schedule = state
                        .round_schedule(evidence.span)
                        .ok_or_else(|| format!("round equivocation names span {}, which has no schedule", evidence.span))?;
                    palw_execution_permit_of_v2(
                        schedule,
                        evidence.round,
                        lane.width_of_span_len(evidence.span, span_daa),
                        evidence.permit_index,
                        &evidence.bond,
                        self.palw_round_permits_are_tickets_at(point.daa_score),
                    )
                    .ok_or_else(|| {
                        format!(
                            "span {}'s schedule does not grant round {} permit {} to the bond the evidence names",
                            evidence.span, evidence.round, evidence.permit_index
                        )
                    })?;
                    let record = state
                        .bond(&evidence.bond)
                        .ok_or_else(|| "round equivocation names a bond this chain does not have".to_string())?;
                    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                        self.network_id_bytes.as_slice(),
                        Some(self.genesis.hash),
                    );
                    evidence
                        .verify(domain, self.genesis.timestamp, &record.pubkey, Self::verify_mldsa87_with_context_bool)
                        .map_err(|e| format!("round {} permit {}: {e}", evidence.round, evidence.permit_index))?;
                    for span in [evidence.span.saturating_sub(1), evidence.span, evidence.span + 1] {
                        if state.round_equivocated(span, evidence.round, evidence.permit_index) {
                            return Err(format!("round {} permit {} was burned already", evidence.round, evidence.permit_index));
                        }
                    }
                }
                // **ADR-0100 Decision 4: per-shard licensing's three objects.** The fence first, the
                // same doubled-fence doctrine as every court move; then each object's authority:
                // the class's registrant over a plan, the bond over its shard list, and over a part
                // the seats' own receipts, checked exactly as a whole licence checks them, over
                // that shard's seats.
                Obj::ClassShardPlanDeclared { class_id, shard_count, signature } => {
                    if !self.palw_shard_licensing_at(point.daa_score) {
                        return Err(format!("class {class_id}: per-shard licensing is not armed on this network (ADR-0100)"));
                    }
                    let class = state
                        .class(class_id)
                        .ok_or_else(|| format!("a shard plan names class {class_id} this chain does not have"))?;
                    let registrant = class
                        .registrant_bond
                        .ok_or_else(|| format!("class {class_id} was registered at genesis and has no registrant"))?;
                    let record = state.bond(&registrant).ok_or_else(|| format!("class {class_id}'s registrant bond is gone"))?;
                    let message = kaspa_consensus_core::palw_shard_licensing_v1::palw_class_shard_plan_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        class_id,
                        *shard_count,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_shard_licensing_v1::PALW_SHARD_PLAN_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("class {class_id}'s shard plan is not signed by its registrant"));
                    }
                }
                // **RFC-0006: layer-sharded panels' three objects** (tags 91-93). The fence first, the doubled-fence doctrine of
                // every court move; then each object's authority: the class's registrant over a plan, the seats' own cell-masked
                // receipts over a part (checked exactly as a whole licence checks theirs, over that shard's slice, by
                // `validate_tir_shard_part_v1`), the bond over a shard possession proof.
                Obj::TirShardPlanDeclared { class_id, s_l, s_p, signature } => {
                    if !self.palw_tir_shard_at(point.daa_score) {
                        return Err(format!("class {class_id}: layer-sharded panels are not armed on this network (RFC-0006)"));
                    }
                    let class = state
                        .class(class_id)
                        .ok_or_else(|| format!("a layer-shard plan names class {class_id} this chain does not have"))?;
                    let registrant = class
                        .registrant_bond
                        .ok_or_else(|| format!("class {class_id} was registered at genesis and has no registrant"))?;
                    let record = state.bond(&registrant).ok_or_else(|| format!("class {class_id}'s registrant bond is gone"))?;
                    let message = kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_plan_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        class_id,
                        *s_l,
                        *s_p,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_tir_shard_v1::PALW_TIR_SHARD_PLAN_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("class {class_id}'s layer-shard plan is not signed by its registrant"));
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
                Obj::TirSeatReadinessProved { bond, class_id, shard, span, proof, signature } => {
                    if !self.palw_tir_shard_at(point.daa_score) {
                        return Err(format!(
                            "a shard possession proof for class {class_id} below layer-sharded panels' fence (RFC-0006)"
                        ));
                    }
                    if !self.palw_model_registry_at(point.daa_score) || !self.palw_readiness_v2_at(point.daa_score) {
                        return Err(format!(
                            "a shard possession proof for class {class_id} below the registry's readiness V2 (ADR-0133)"
                        ));
                    }
                    let record = state
                        .bond(bond)
                        .ok_or_else(|| format!("a shard possession proof names bond {bond:?} this chain does not have"))?;
                    let plan =
                        state.tir_shard_plan(class_id).ok_or_else(|| format!("class {class_id} declared no layer-shard plan"))?;
                    let opened_leaf_hashes: Vec<(u32, kaspa_hashes::Hash64)> = proof
                        .opened
                        .iter()
                        .map(|(index, operand)| (*index, kaspa_consensus_core::palw_artifact::artifact_leaf_v1(operand)))
                        .collect();
                    let message = kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_readiness_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        bond,
                        class_id,
                        plan.s_l,
                        *shard,
                        *span,
                        &opened_leaf_hashes,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_tir_shard_v1::PALW_TIR_SHARD_READINESS_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s shard possession proof is not signed by the key it registered"));
                    }
                }
                Obj::BondShardsDeclared { bond, class_id, shard_count, shards, signature } => {
                    if !self.palw_shard_licensing_at(point.daa_score) {
                        return Err(format!("bond {bond:?}: per-shard licensing is not armed on this network (ADR-0100)"));
                    }
                    let record =
                        state.bond(bond).ok_or_else(|| format!("a shard list names bond {bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_shard_licensing_v1::palw_bond_shards_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        bond,
                        class_id,
                        *shard_count,
                        shards,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_shard_licensing_v1::PALW_BOND_SHARDS_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {bond:?}'s shard list is not signed by the bond"));
                    }
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
                Obj::FreePromptCommitted { .. } => {}
                // **RFC-0003 §I.4: a tensor claim's commitment** is a free-prompt commitment of job version 10:
                // the acceptance walk built it only past both fences, from a payload whose signature it verified
                // under the key the job carries, and the fold derives the rest (the class, the work, the capacity).
                Obj::GenTensorCommitted { .. } => {}
                // **RFC-0003 (tag 68): a generative class registration.** Below `palw_gen_v1` it is
                // dropped by name, exactly as an older build skips the payload it cannot decode (A-2;
                // the walk dropped it first). Past it, the IR arm's order: the chain's target (one map
                // read), the registrant's signature, then the pipeline admission — which decodes the
                // class, counts its step tree and prices its court — and its share rule (0‰: no
                // attempt lane exists for pipelines).
                Obj::ClassRegisteredGenV1 { class_id, .. } => {
                    if !self.palw_gen_at(point.daa_score) {
                        return Err(format!(
                            "generative class {class_id} is refused: palw_gen_v1 is not in force at this block (RFC-0003)"
                        ));
                    }
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err(format!("generative class {class_id} registered on a network with no V2 bundle"));
                    };
                    self.palw_v2_class_registration_starts_at_the_chains_target(state, object)?;
                    self.palw_v2_class_registration_is_signed(state, object)?;
                    // The k-ary court at this block, resolved as the IR arm resolves it: a cone that
                    // reduces over the history is dissected only under it (F7 composed).
                    let court = match self.palw_kary_court_active_at(point.daa_score) {
                        false => None,
                        true => {
                            let derived = self
                                .palw_court_params_at(point.daa_score)
                                .ok_or_else(|| format!("generative class {class_id} is registered on a chain with no V2 ruleset"))?
                                .map_err(|e| format!("generative class {class_id} is judged under a court with no shape: {e}"))?;
                            Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                dissection_arity: derived.dissection_arity(),
                                prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                window_court_daa: bundle.state.window_court(),
                            })
                        }
                    };
                    let rules = kaspa_consensus_core::palw_gen_admission_v1::PalwGenAdmissionRulesV1 {
                        fence: self.palw_gen_v1.expect("palw_gen_at said the fence is in force"),
                        court,
                        held_armed: self.palw_held_context_at(point.daa_score),
                        // RFC-0003 decision 22, read off the bundle's mirror: with the held leaf challenge a close up to
                        // the carried cap is convictable under the held regime (PALW-GEN-21's bound lifts).
                        held_close_chunks: self.palw_held_close_chunks_at(point.daa_score),
                        // The block's box-demand rules, as the IR arm reads them (the fold's mirror).
                        demand: kaspa_consensus_core::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(&bundle.state, point.daa_score),
                        // The close-sizing twin, read off the bundle's mirror (`palw_gen_range_twin_v1`).
                        twin: if bundle.state.gen_range_twin_active_at(point.daa_score) {
                            kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1::Range
                        } else {
                            kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1::Element
                        },
                        // The task-head profile at the block: tag 6 is a `Head` class only where it is in force.
                        heads: self.palw_task_heads_v1.filter(|fence| fence.activation.is_active(point.daa_score)),
                    };
                    kaspa_consensus_core::palw_gen_admission_v1::verify_gen_class_admission_v1(bundle, &rules, object)
                        .map_err(|e| format!("generative class {class_id} is not admissible: {e}"))?;
                }
                // **RFC-0002 Phase F (tag 61): an IR class registration.** Below `palw_tir_v1` it is
                // refused by name (the walk drops it first, exactly as an older build skips the payload
                // it cannot decode, A-2); above it, a bought registration (a slot and the 1 MSK burn)
                // judged in the legacy arm's order and then by admission v10 — at most one a block
                // reaches here (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`, the walk's by-name drop).
                Obj::ClassRegisteredTirV1 { class_id, share_permille, admission, .. } => {
                    if !self.palw_tir_at(point.daa_score) {
                        return Err(format!(
                            "IR class {class_id} is refused: palw_tir_v1 is not in force at this block (RFC-0002 Phase F)"
                        ));
                    }
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err(format!("IR class {class_id} registered on a network with no V2 bundle"));
                    };
                    // The legacy arm's order, the graph gate last: the chain's target (one map
                    // read), the registrant's signature, then admission v10 — which decodes the
                    // program and derives the primitives the share rule reads.
                    self.palw_v2_class_registration_starts_at_the_chains_target(state, object)?;
                    self.palw_v2_class_registration_is_signed(state, object)?;
                    // RFC-0002 F7: the k-ary court at this block, resolved as the legacy arm resolves
                    // it — a cone that reduces over the history is dissected only under it.
                    let court = match self.palw_kary_court_active_at(point.daa_score) {
                        false => None,
                        true => {
                            let derived = self
                                .palw_court_params_at(point.daa_score)
                                .ok_or_else(|| format!("IR class {class_id} is registered on a chain with no V2 ruleset"))?
                                .map_err(|e| format!("IR class {class_id} is judged under a court with no shape: {e}"))?;
                            let window = kaspa_consensus_core::palw_class_admission_v2::palw_court_window_for_history_v1(
                                bundle.state.window_court(),
                                self.palw_model_court_window_active_at(point.daa_score),
                                self.palw_held_context_at(point.daa_score),
                                &derived,
                                admission.class.layout.max_context as u64,
                                admission.class.layout.h_tile.max(1),
                            )
                            .map_err(|e| format!("IR class {class_id}: {e}"))?;
                            Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                dissection_arity: derived.dissection_arity(),
                                prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                window_court_daa: window,
                            })
                        }
                    };
                    let rules = kaspa_consensus_core::palw_tir_admission_v1::PalwTirAdmissionRulesV1 {
                        fence: self.palw_tir_v1.expect("palw_tir_at said the fence is in force"),
                        held: kaspa_consensus_core::palw_class_admission_v2::PalwHeldAdmissionV1 {
                            armed: self.palw_held_context_at(point.daa_score),
                            panel_da: self.palw_panel_da_at(point.daa_score),
                        },
                        prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                        court,
                        demand: kaspa_consensus_core::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(&bundle.state, point.daa_score),
                        model_court_window_active: self.palw_model_court_window_active_at(point.daa_score),
                    };
                    // The network's committed certified families and the chain's own, as the legacy
                    // arm reads them (consensus never reads the drilled registry).
                    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
                    let chain_certified =
                        state.chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt);
                    let (entry, _) = kaspa_consensus_core::palw_tir_admission_v1::verify_class_admission_v10(
                        bundle,
                        &rules,
                        object,
                        &certified,
                        &chain_certified,
                    )
                    .map_err(|e| format!("IR class {class_id} is not admissible: {e}"))?;
                    // Decision H's and ADR-0069's share rule, as the legacy arm states it: the
                    // minimum grantable share for a class a certified family covers (below
                    // `palw_admission_independence`), nothing otherwise.
                    let prosecutable = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
                        bundle.court_e2e_root,
                        &certified,
                        &chain_certified,
                        &entry.reachable_kernels,
                    )
                    .map_err(|e| format!("IR class {class_id}: {e}"))?
                    .is_some();
                    let independence = self.palw_admission_independence_at(point.daa_score);
                    let required = if prosecutable && !independence { state_params.min_grantable_share_permille() } else { 0 };
                    if *share_permille != required {
                        return Err(format!(
                            "IR class {class_id} registers at {share_permille}‰; the share rule requires {required}‰ (ADR-0049 \
                             Decision H, ADR-0069 Decision 6, ADR-0145 §7)"
                        ));
                    }
                }
                // ADR-0075: both certification objects are judged entirely by the transition —
                // the evidence by the court's grader, the class binding by the class's own profile
                // hash and kernel coverage — and neither needs a signature, a bundle or a store.
                Obj::FamilyCertified { .. } | Obj::ClassLaneCertified { .. } => {}
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
                // RFC-0002 Phase F (tag 63): judged by the transition like the legacy lane object;
                // below `palw_tir_v1` dropped by name (the acceptance walk drops it first).
                Obj::ClassLaneCertifiedTirV1 { class_id, .. } => {
                    if !self.palw_tir_at(point.daa_score) {
                        return Err(format!("IR class {class_id}'s lane certification is refused: palw_tir_v1 is not in force"));
                    }
                }
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
                        return Err(format!(
                            "{} is refused: palw_improvement_v1 is not in force at this block (RFC-0004)",
                            "a rollback"
                        ));
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
                // **Lane LG14-A (tags 154–155): the dispute reservation and its release** — the fence, an Active signer, its signature
                // over the payload; the claim, the caps, the deposit and the hold are the fold's.
                Obj::DisputeReservedV1 { reservation, signature } => {
                    let payload = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reserved_payload_v1(reservation);
                    self.palw_legacy_dispute_signature_ok(
                        state,
                        point.daa_score,
                        kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RESERVED_TAG_V1,
                        &reservation.reserver,
                        &payload,
                        signature,
                    )?;
                }
                Obj::DisputeReacquiredV1 { request, signature } => {
                    let payload = borsh::to_vec(request.as_ref()).map_err(|e| e.to_string())?;
                    let court = self.palw_court_params_v2.as_ref().ok_or("public reacquisition needs V2 court parameters")?;
                    if payload.len() as u64 > court.max_close_bytes() {
                        return Err("public reacquisition exceeds the ruleset's close carriage ceiling".into());
                    }
                    if let kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(unit) = request.unit
                        && matches!(
                            unit,
                            kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::PromptIds { .. }
                                | kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::FpJob
                        )
                    {
                        let binding = request.binding.as_ref().ok_or("input/job reacquisition needs its binding")?;
                        let params = self.palw_state_params_v2.as_ref().ok_or("input/job reacquisition needs V2 state parameters")?;
                        kaspa_consensus_core::palw_legacy_answer_chunks_v2::palw_legacy_input_answer_transport_check_v2(
                            binding,
                            &unit,
                            self.palw_prompt_ids_form_at(point.daa_score),
                            court.max_close_bytes(),
                            kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(params),
                        )?;
                    }
                    self.palw_legacy_dispute_signature_ok(
                        state,
                        point.daa_score,
                        kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_REACQUIRE_TAG_V1,
                        &request.reserver,
                        &payload,
                        signature,
                    )?;
                }
                Obj::DisputeReleasedV1 { claim, reserver, signature } => {
                    let payload = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_released_payload_v1(claim);
                    self.palw_legacy_dispute_signature_ok(
                        state,
                        point.daa_score,
                        kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RELEASED_TAG_V1,
                        reserver,
                        &payload,
                        signature,
                    )?;
                }
                // **Lane LG14-B (tags 157–159): the legacy held DA** — the fence, the named bond's signature over the object's message,
                // the close ceiling on what it carries; the units, answers and verdicts are the fold's.
                Obj::LegacyHeldDemandedV2 { demand } => {
                    use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                    let domain = self.palw_network_domain_v2();
                    let message = l::palw_legacy_held_demand_message_v2(domain.as_byte_slice(), demand);
                    let bytes = borsh::to_vec(&demand.binding).map(|b| b.len() as u64).unwrap_or(u64::MAX);
                    if matches!(demand.unit, l::PalwLegacyHeldUnitV2::PromptIds { .. } | l::PalwLegacyHeldUnitV2::FpJob) {
                        let court = self.palw_court_params_v2.as_ref().ok_or("input/job disclosure needs V2 court parameters")?;
                        let params = self.palw_state_params_v2.as_ref().ok_or("input/job disclosure needs V2 state parameters")?;
                        kaspa_consensus_core::palw_legacy_answer_chunks_v2::palw_legacy_input_answer_transport_check_v2(
                            &demand.binding,
                            &demand.unit,
                            self.palw_prompt_ids_form_at(point.daa_score),
                            court.max_close_bytes(),
                            kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(params),
                        )?;
                    }
                    self.palw_legacy_held_signature_ok(
                        state,
                        point.daa_score,
                        &demand.claim,
                        &demand.accuser,
                        &message,
                        &demand.signature,
                        l::PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2,
                        bytes,
                    )?;
                }
                Obj::LegacyHeldAnsweredV2 { answer } => {
                    use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                    let domain = self.palw_network_domain_v2();
                    let message = l::palw_legacy_held_answer_message_v2(domain.as_byte_slice(), answer);
                    let mut bytes = l::palw_legacy_held_answer_bytes_v2(&answer.answer);
                    if matches!(answer.unit, l::PalwLegacyHeldUnitV2::PromptIds { .. } | l::PalwLegacyHeldUnitV2::FpJob) {
                        bytes = bytes.saturating_add(borsh::to_vec(&answer.binding).map_err(|e| e.to_string())?.len() as u64);
                        if bytes > kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64 {
                            return Err("the input answer cannot fit a single carrier".into());
                        }
                    }
                    self.palw_legacy_held_signature_ok(
                        state,
                        point.daa_score,
                        &answer.claim,
                        &answer.discloser,
                        &message,
                        &answer.signature,
                        l::PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2,
                        bytes,
                    )?;
                }
                Obj::PublicCourtStreamOpenedV1 { seed } => {
                    use kaspa_consensus_core::palw_constraint_stream_carriage_v1 as stage;
                    // A-2: below its owning fence (`palw_staged_public_court_v1`) the kind rides unjudged, as int-12 reads it.
                    if !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score)) {
                        continue;
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("staged court needs V2 carrier parameters")?;
                    seed.check(court.max_close_bytes())?;
                    let owner = state.bond(&seed.reserver).ok_or("staged owner has no registered bond")?;
                    let bundle = match state.staged_owned_row_v1(seed.claim, seed.reserver) {
                        Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(bundle)) => {
                            Some(bundle)
                        }
                        _ => None,
                    };
                    let authorized = match bundle {
                        Some(bundle) => {
                            bundle.reserved_daa == seed.reserved_daa
                                && bundle.opened.len() < bundle.plan.stages.len()
                                && bundle.opened.len() == bundle.active.map_or(0, |n| n as usize + 1)
                                && point.daa_score >= bundle.plan.terms.accepted_daa
                                && point.daa_score < bundle.plan.stages[bundle.opened.len()].terms.open_until_daa
                        }
                        None => state
                            .legacy_dispute_reservation_v1(&seed.claim, &seed.reserver)
                            .is_some_and(|reservation| reservation.reserved_daa == seed.reserved_daa),
                    };
                    if !authorized || !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err("staged move changes live reservation identity or owner is inactive".into());
                    }
                    if !Self::verify_mldsa87_with_context_bool(
                        &owner.pubkey,
                        &seed.signing_message(),
                        &seed.signature,
                        stage::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                    ) {
                        return Err("staged move is not signed by its literal reserver".into());
                    }
                }
                Obj::PublicCourtStreamLeafV1 { leaf } => {
                    use kaspa_consensus_core::palw_constraint_stream_carriage_v1 as stage;
                    // A-2: below its owning fence (`palw_staged_public_court_v1`) the kind rides unjudged, as int-12 reads it.
                    if !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score)) {
                        continue;
                    }
                    let _court = self.palw_court_params_v2.as_ref().ok_or("staged court needs V2 carrier parameters")?;
                    leaf.check()?;
                    let owner = state.bond(&leaf.reserver).ok_or("staged owner has no registered bond")?;
                    let paid = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_paid_authority_v1(
                        &state,
                        (leaf.claim, leaf.reserver),
                    )?;
                    let authorized = match paid {
                        Some(paid) => {
                            paid.reserved_daa == leaf.reserved_daa
                                && paid.seed_id == leaf.seed_id
                                && point.daa_score >= paid.terms.accepted_daa
                                && point.daa_score < paid.terms.stream_until_daa
                                && point.daa_score < paid.terms.material_until_daa
                        }
                        // A seed earlier in the same block is checked by the ordered fold.
                        None => state
                            .legacy_dispute_reservation_v1(&leaf.claim, &leaf.reserver)
                            .is_some_and(|r| r.reserved_daa == leaf.reserved_daa),
                    };
                    if !authorized || !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err("staged move changes live reservation identity or owner is inactive".into());
                    }
                    if !Self::verify_mldsa87_with_context_bool(
                        &owner.pubkey,
                        &leaf.signing_message(),
                        &leaf.signature,
                        stage::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                    ) {
                        return Err("staged move is not signed by its literal reserver".into());
                    }
                }
                Obj::PublicCourtStreamMoveV1 { movement } => {
                    use kaspa_consensus_core::palw_constraint_stream_carriage_v1 as stage;
                    // A-2: below its owning fence (`palw_staged_public_court_v1`) the kind rides unjudged, as int-12 reads it.
                    if !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score)) {
                        continue;
                    }
                    let _court = self.palw_court_params_v2.as_ref().ok_or("staged court needs V2 carrier parameters")?;
                    movement.check()?;
                    let owner = state.bond(&movement.reserver).ok_or("staged owner has no registered bond")?;
                    let paid = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_paid_authority_v1(
                        &state,
                        (movement.claim, movement.reserver),
                    )?;
                    let artifact = match &movement.action {
                        stage::PalwConstraintStreamActionV1::HistoryCustodyGrant { grant, receipt } => {
                            let contract = match state.staged_owned_row_v1(movement.claim, movement.reserver) {
                                _ => state
                                    .staged_claim_contract_v1(&movement.claim)
                                    .ok_or("history grant has no accepted material contract")?,
                            };
                            let signer = state.bond(&grant.seat).ok_or("history grant signer is not registered")?;
                            grant.validate_signature_v1(receipt, &signer.pubkey, Self::verify_mldsa87_with_context_bool)?;
                            Some(
                                movement.reserver == grant.seat
                                    && movement.claim == grant.claim
                                    && movement.reserved_daa == contract.terms.accepted_daa
                                    && point.daa_score >= contract.terms.accepted_daa
                                    && point.daa_score < contract.terms.material_until_daa
                                    && contract
                                        .prefix_custody
                                        .as_ref()
                                        .is_some_and(|custody| custody.matches_credited_receipt_v1(movement.claim, receipt)),
                            )
                        }
                        stage::PalwConstraintStreamActionV1::ArtifactInventoryOpen { offer, .. } => {
                            offer.validate_signature_v1(&owner.pubkey, Self::verify_mldsa87_with_context_bool)?;
                            Some(
                                movement.claim == offer.artifact_hash
                                    && movement.reserver == offer.opener
                                    && movement.seed_id == offer.id_v1()
                                    && movement.reserved_daa == 0
                                    && point.daa_score < offer.accept_until_daa,
                            )
                        }
                        stage::PalwConstraintStreamActionV1::ArtifactInventoryMove(_) => {
                            Some(state.staged_tokenizer_inventory_open_v1(movement.claim, movement.reserver).is_some_and(|row| {
                                movement.seed_id == row.offer.id_v1()
                                    && movement.reserved_daa == row.opened_daa
                                    && point.daa_score >= row.opened_daa
                                    && point.daa_score < row.until_daa
                            }))
                        }
                        _ => None,
                    };
                    let authorized = if let Some(artifact) = artifact {
                        artifact
                    } else {
                        match paid {
                            Some(paid) => {
                                let control_terms =
                                    match (&movement.action, state.staged_owned_row_v1(movement.claim, movement.reserver)) {
                                        (
                                            stage::PalwConstraintStreamActionV1::ActivateStage { .. }
                                            | stage::PalwConstraintStreamActionV1::InventoryPart { .. }
                                            | stage::PalwConstraintStreamActionV1::InventoryFinish
                                            | stage::PalwConstraintStreamActionV1::SelectedEventDemand { .. }
                                            | stage::PalwConstraintStreamActionV1::SelectedEventAnswer(_)
                                            | stage::PalwConstraintStreamActionV1::SelectedEventCollect { .. }
                                            | stage::PalwConstraintStreamActionV1::HistoryRecovery { .. },
                                            Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(
                                                bundle,
                                            )),
                                        ) => bundle.plan.terms,
                                        _ => paid.terms,
                                    };
                                let until = if matches!(
                                    movement.action,
                                    stage::PalwConstraintStreamActionV1::PrefixPublicCollect { .. }
                                        | stage::PalwConstraintStreamActionV1::SelectedEventCollect { .. }
                                        | stage::PalwConstraintStreamActionV1::HistoryRecovery {
                                            action:
                                                kaspa_consensus_core::palw_history_custody_v1::PalwHistoryRecoveryActionV1::Collect,
                                            ..
                                        }
                                ) {
                                    control_terms.collect_until_daa
                                } else {
                                    control_terms.stream_until_daa
                                };
                                let literal_seed =
                                    match (&movement.action, state.staged_owned_row_v1(movement.claim, movement.reserver)) {
                                        (
                                            stage::PalwConstraintStreamActionV1::SelectedEventDemand { .. }
                                            | stage::PalwConstraintStreamActionV1::SelectedEventAnswer(_)
                                            | stage::PalwConstraintStreamActionV1::SelectedEventCollect { .. },
                                            Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(
                                                bundle,
                                            )),
                                        ) => bundle.plan.id_v1(bundle.claim),
                                        (
                                            stage::PalwConstraintStreamActionV1::HistoryRecovery { .. },
                                            Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedCourtRowV1::OwnerBundle(
                                                bundle,
                                            )),
                                        ) => bundle.plan.id_v1(bundle.claim),
                                        _ => paid.seed_id,
                                    };
                                paid.reserved_daa == movement.reserved_daa
                                    && literal_seed == movement.seed_id
                                    && point.daa_score >= paid.terms.accepted_daa
                                    && point.daa_score < until
                                    && point.daa_score < paid.terms.material_until_daa
                            }
                            None => state
                                .legacy_dispute_reservation_v1(&movement.claim, &movement.reserver)
                                .is_some_and(|r| r.reserved_daa == movement.reserved_daa),
                        }
                    };
                    if !authorized || !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err("staged move changes live reservation identity or owner is inactive".into());
                    }
                    // Only a typed public-prefix Answer has distinct signature authority.
                    // The fold additionally checks its saved literal custody/owner slot.
                    let signer = state.bond(&movement.authorization_bond_v1()).ok_or("staged answerer has no registered bond")?;
                    if !Self::verify_mldsa87_with_context_bool(
                        &signer.pubkey,
                        &movement.signing_message(),
                        &movement.signature,
                        stage::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                    ) {
                        return Err("staged move is not signed by its literal authorized owner/answerer".into());
                    }
                }
                Obj::ObjectiveEvidenceDeclaredV1 { declaration } => {
                    use kaspa_consensus_core::palw_terminal_carriage_v1 as t;
                    // A-2: below its owning fence (`palw_legacy_public_filer_v1`) the kind rides unjudged, as int-12 reads it; past it, the
                    // objective-offence and attribution fences are this kind's own rule.
                    if !self.palw_legacy_public_filer_at(point.daa_score) {
                        continue;
                    }
                    if !self.palw_objective_offence_at(point.daa_score) || !self.palw_offence_attribution_at(point.daa_score) {
                        return Err("terminal evidence is below its objective-offence/attribution fence".into());
                    }
                    let court = self.palw_court_params_v2.as_ref().ok_or("terminal evidence needs V2 court parameters")?;
                    declaration.check(court.max_close_bytes())?;
                    let owner = state.bond(&declaration.reserver).ok_or("terminal reserver has no registered bond")?;
                    let reservation = state
                        .legacy_dispute_reservation_v1(&declaration.claim, &declaration.reserver)
                        .ok_or("terminal reserver has no live reservation")?;
                    if reservation.reserved_daa != declaration.reserved_daa {
                        return Err("terminal declaration belongs to another reservation".into());
                    }
                    if !matches!(owner.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err("terminal reserver is not active".into());
                    }
                    let message = t::palw_terminal_message_v1(self.palw_network_domain_v2(), declaration)?;
                    if !Self::verify_mldsa87_with_context_bool(
                        &owner.pubkey,
                        message.as_byte_slice(),
                        &declaration.signature,
                        t::PALW_TERMINAL_CONTEXT_V1,
                    ) {
                        return Err("terminal declaration is not signed by the reserver".into());
                    }
                }
                Obj::ObjectiveEvidenceChunkV1 { part } => {
                    use kaspa_consensus_core::palw_terminal_carriage_v1 as t;
                    // A-2: below its owning fence (`palw_legacy_public_filer_v1`) the kind rides unjudged, as int-12 reads it; past it, the
                    // objective-offence and attribution fences are this kind's own rule.
                    if !self.palw_legacy_public_filer_at(point.daa_score) {
                        continue;
                    }
                    if !self.palw_objective_offence_at(point.daa_score) || !self.palw_offence_attribution_at(point.daa_score) {
                        return Err("terminal evidence is below its objective-offence/attribution fence".into());
                    }
                    let row =
                        state.terminal_pending_v1(&(part.claim, part.reserver)).ok_or("terminal part has no owned declaration")?;
                    let court = self.palw_court_params_v2.as_ref().ok_or("terminal evidence needs V2 court parameters")?;
                    if let Some(evidence) = row.preview(part, court.max_close_bytes())? {
                        let d = &row.declaration;
                        if matches!(d.kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::TirIdentityMismatch)
                            && !self.palw_tir_at(point.daa_score)
                        {
                            return Err("terminal IR identity proof below its IR fence".into());
                        }
                        if t::palw_terminal_is_constraint_v1(d.kind, &evidence)
                            && !state_params.fp_decode_constraint_active_at(point.daa_score)
                        {
                            return Err("terminal constraint proof below its constraint fence".into());
                        }
                        if !state_params.capacity_batch_active_at(point.daa_score)
                            && kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_evidence_is_windowed_v1(&evidence)
                        {
                            return Err("terminal Windowed receipt below its batch fence".into());
                        }
                        let accused = state.bond(&d.accused).ok_or("terminal accused has no registered bond")?;
                        t::palw_terminal_adjudicate_v1(
                            state,
                            d,
                            &evidence,
                            court.max_close_bytes(),
                            state_params.fp_decode_rules_at(point.daa_score),
                            self.palw_identity_rules_v1(point.daa_score),
                            self.palw_session_rule_at(point.daa_score),
                            Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidSigCheckV1 {
                                chain_domain: self.palw_network_domain_v2(),
                                seat_pubkey: &accused.pubkey,
                                seat_active: matches!(
                                    accused.status,
                                    kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active
                                        | kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Retiring { .. }
                                ),
                                verify: &Self::verify_mldsa87_with_context_bool,
                            }),
                        )?;
                    }
                }
                Obj::LegacyHeldAnswerDeclaredV2 { declaration } => {
                    use kaspa_consensus_core::palw_legacy_answer_chunks_v2 as chunks;
                    let court = self.palw_court_params_v2.as_ref().ok_or("multipart DA answer needs V2 court parameters")?;
                    declaration.check(court.max_close_bytes())?;
                    let domain = self.palw_network_domain_v2();
                    let message = chunks::palw_legacy_answer_declaration_message_v2(domain.as_byte_slice(), declaration);
                    self.palw_legacy_held_signature_ok(
                        state,
                        point.daa_score,
                        &declaration.claim,
                        &declaration.discloser,
                        &message,
                        &declaration.signature,
                        chunks::PALW_LEGACY_ANSWER_DECLARE_CONTEXT_V2,
                        borsh::to_vec(declaration).map_err(|e| e.to_string())?.len() as u64,
                    )?;
                }
                Obj::LegacyHeldAnswerChunkV2 { chunk } => {
                    use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                    if !self.palw_legacy_held_da_v2_at(point.daa_score) {
                        return Err("multipart DA answer is below palw_legacy_held_da_v2".into());
                    }
                    let row = state.legacy_answer_pending_v2(&chunk.key()).ok_or("no signed multipart DA declaration")?;
                    let court = self.palw_court_params_v2.as_ref().ok_or("multipart DA answer needs V2 court parameters")?;
                    row.declaration.check(court.max_close_bytes())?;
                    if let Some(answer) = row.preview(chunk)? {
                        let domain = self.palw_network_domain_v2();
                        let message = l::palw_legacy_held_answer_message_v2(domain.as_byte_slice(), &answer);
                        self.palw_legacy_held_signature_ok(
                            state,
                            point.daa_score,
                            &answer.claim,
                            &answer.discloser,
                            &message,
                            &answer.signature,
                            l::PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2,
                            row.declaration.total_bytes,
                        )?;
                    }
                }
                Obj::LegacyLeafRecomputedV2 { accusation } => {
                    use kaspa_consensus_core::palw_legacy_held_da_v2 as l;
                    let domain = self.palw_network_domain_v2();
                    let message = l::palw_legacy_leaf_recompute_message_v2(domain.as_byte_slice(), accusation);
                    let bytes = l::palw_legacy_leaf_recompute_bytes_v2(accusation);
                    self.palw_legacy_held_signature_ok(
                        state,
                        point.daa_score,
                        &accusation.claim,
                        &accusation.accuser_bond,
                        &message,
                        &accusation.signature,
                        l::PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2,
                        bytes,
                    )?;
                }
                // **Lane BUDGET (tag 140): a bond's capital assignment** — the fence, an Active signer, its signature; ADR-0177 D3's rules
                // are the fold's.
                Obj::ClassSemanticsDeclaredV1 { declaration, signature } => {
                    use kaspa_consensus_core::palw_class_semantics_v1 as semantics;
                    if !self.palw_class_semantics_v1.is_some_and(|f| f.is_active(point.daa_score)) {
                        return Err("semantic declaration is below palw_class_semantics_v1".into());
                    }
                    declaration.check().map_err(str::to_string)?;
                    let class = state.class(&declaration.class_id).ok_or("semantic declaration names an unknown class")?;
                    if class.artifact_root != declaration.artifact_root {
                        return Err("semantic declaration names another artifact".into());
                    }
                    let registrant = class
                        .registrant_bond
                        .as_ref()
                        .ok_or("a sentinel registration has no signing registrant for prospective migration")?;
                    let bond = state.bond(registrant).ok_or("the signing registrant is not a public bond")?;
                    if !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err("the signing registrant is not Active".into());
                    }
                    let message = semantics::palw_class_semantics_message_v1(self.palw_network_domain_v2(), declaration);
                    if !Self::verify_mldsa87_with_context_bool(
                        &bond.pubkey,
                        message.as_byte_slice(),
                        signature,
                        semantics::PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1,
                    ) {
                        return Err("the original class registrant did not sign this prospective semantic declaration".into());
                    }
                }
                Obj::BondCapitalAssignedV1 { bond, assignments, sequence, signature } => {
                    self.palw_capital_assignment_signature_ok(state, point.daa_score, bond, assignments, *sequence, signature)?;
                }
                // (tag 108): the acceptance walk replaces the envelope by its registration before this gate; one that reaches it was
                // not unwrapped (a direct caller of the gate), and is refused.
                Obj::SignedRegistrationV1 { .. } => {
                    return Err("a signed registration envelope is unwrapped by the acceptance walk, not judged as itself".to_string());
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
                Obj::AdapterClassListed { payload, lister, signature } => {
                    if !self.palw_adapter_class_at(point.daa_score) {
                        return Err("an adapter class listing is refused: palw_adapter_class_v1 is not in force at this block (RFC-0001 §2.10)".to_string());
                    }
                    self.palw_adapter_listing_is_signed(state, payload.as_ref(), lister, signature)?;
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err("an adapter class listing on a network with no V2 bundle".to_string());
                    };
                    let rules = kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeAdmissionV1 {
                        court: self.palw_kary_court_active_at(point.daa_score),
                        carriable: kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court),
                        work_cap: kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
                    };
                    state
                        .adapter_class_acceptance_v1(payload, rules)
                        .map_err(|why| format!("adapter class {} is not admissible: {why} (RFC-0001 §2.10)", payload.class_id))?;
                }
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
                        return Err(format!(
                            "the derivation of claim {} is not signed by the executor key it declares",
                            object.claim_id
                        ));
                    }
                }
                // Phase F's reserved tags 84 and 85 carry an uninhabited payload: no such object can be decoded.
                Obj::ReservedPipelineDa84(never) | Obj::ReservedPipelineDa85(never) => match *never {},
            }
        }
        Ok(())
    }
}
