//! Object authentication and admission for the existing bond_readiness family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_bond_readiness_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
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
                Obj::BondCapitalAssignedV1 { bond, assignments, sequence, signature } => {
                    self.palw_capital_assignment_signature_ok(state, point.daa_score, bond, assignments, *sequence, signature)?;
                }
            _ => unreachable!("bond_readiness validator received another object family"),
        }
        Ok(())
    }
}
