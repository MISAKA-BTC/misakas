//! **One consensus object applied to the fold (apply_object)** — moved out of `palw_state_v2.rs` (2026-10-12, a behaviour-preserving split: the items are the same; members the parent calls are `pub(super)`).

use super::*;

pub(super) fn apply_object(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
) -> Result<(), PalwStateV2Error> {
    // Lane g14-court-capacity (audit G-07): past `palw_court_capacity_v1` every court operation is charged its deterministic cost
    // against the block's court budget before its arm runs. A no-op below the fence.
    palw_court_capacity_fold_v1::charge_court_op_v1(builder, ctx, object)?;
    let paired = matches!(object, PalwConsensusObjectV2::FreePromptCommitted { claim, .. }
        if builder.extras.tir_public_originals.contains_key(claim));
    if !paired {
        return apply_object_inner(builder, ctx, object);
    }
    // A prospective original is ONE native object. Failure after the ordinary
    // FP reservation/budget writes cannot leave an accepted unprotected claim.
    // `checkpoint()` is test-only on the integration line; production rollback goes through the native resource reservation.
    let saved = builder.try_checkpoint()?;
    let result = match palw_tir_public_fold_v1::check_paired_original_v1(builder, object) {
        Ok(()) => apply_object_inner(builder, ctx, object),
        Err(why) => Err(why),
    };
    if result.is_err() {
        builder.restore(saved);
    }
    result
}

fn apply_object_inner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
) -> Result<(), PalwStateV2Error> {
    object_preconditions_v1(&builder.state, builder.params, builder.extras, ctx, object, |class_id| {
        builder
            .entries
            .iter()
            .any(|entry| matches!(entry, PalwDeltaEntryV2::Class { key, old: None, new: Some(_) } if key == class_id))
    })?;
    let check_receipt =
        |id: &Hash64| crate::palw_panel_lifetime_v2::require_panel_receipt_time_v2(&builder.state, builder.params, id, ctx.daa_score);
    match object {
        PalwConsensusObjectV2::ReceiptLicensed { claim, .. }
        | PalwConsensusObjectV2::ReceiptLicensedV2 { claim, .. }
        | PalwConsensusObjectV2::OptimisticLicensed { claim, .. } => check_receipt(claim)?,
        PalwConsensusObjectV2::ShardReceiptLicensed { part } => check_receipt(&part.claim)?,
        PalwConsensusObjectV2::TirShardReceiptLicensed { part } => check_receipt(&part.claim)?,
        _ => {}
    }
    // Only the prospective original TIR frame adds this requirement. The
    // ordinary acknowledgement/credit itself occurs atomically with grant and
    // Ready in its separate handler, never by accepting an unqualified licence.
    match object {
        PalwConsensusObjectV2::ReceiptLicensed { claim, receipts } => {
            palw_tir_public_fold_v1::require_qualified_receipts_v1(&builder.state, *claim, receipts)?;
        }
        PalwConsensusObjectV2::ReceiptLicensedV2 { claim, .. } | PalwConsensusObjectV2::OptimisticLicensed { claim, .. } => {
            palw_tir_public_fold_v1::require_qualified_typed_receipts_v1(&builder.state, *claim)?;
        }
        PalwConsensusObjectV2::ShardReceiptLicensed { part } => {
            palw_tir_public_fold_v1::require_qualified_typed_receipts_v1(&builder.state, part.claim)?;
        }
        PalwConsensusObjectV2::TirShardReceiptLicensed { part } => {
            palw_tir_public_fold_v1::require_qualified_typed_receipts_v1(&builder.state, part.claim)?;
        }
        _ => {}
    }
    match object {
        PalwConsensusObjectV2::TirPublicControlV1 { control } => {
            let network = builder.extras.tir_public_network_domain_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
            palw_tir_public_fold_v1::apply_control_v1(builder, ctx, network, control)?;
        }
        PalwConsensusObjectV2::TirPublicGrantReadyV1 { grant } => {
            let network = builder.extras.tir_public_network_domain_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
            palw_tir_public_fold_v1::apply_grant_ready_v1(builder, ctx, network, grant)?;
        }
        // ---- RFC-0010: a certified epoch output (tag 120): queued; the V3 stage verifies it against the branch and drops it if
        // it does not verify, the block standing ----
        PalwConsensusObjectV2::PanelBeaconProofV3 { proof } => {
            palw_panel_v3_fold_v1::queue_beacon_object_v1(builder, proof);
        }
        // ---- RFC-0008 v2: a work-session root declaration (tag 130) ----
        PalwConsensusObjectV2::ExecWorkRootOpenedV2 { declaration } => {
            palw_exec_v2_fold::apply_root_declared_v2(builder, ctx, declaration)?;
        }
        PalwConsensusObjectV2::ExecOriginRecordedV1 { witness } => {
            builder.state.exec_v2_origin_witness_metadata_admissible_v1(builder.params, ctx.daa_score, witness)?;
            builder.exec_origin_budget.charge().map_err(|error| PalwStateV2Error::ExecV2Refused(error.to_string()))?;
            let row = builder.state.exec_v2_origin_witness_admissible_v1(builder.params, ctx.daa_score, witness)?;
            builder.write_exec_origin(witness.claim, Some(row))?;
        }
        // ---- RFC-0007 Part I: the verification vertex (tags 100, 101) ----
        PalwConsensusObjectV2::VerificationVertexV1 { vertex } => {
            palw_vertex_fold_v1::apply_vertex_v1(builder, ctx, vertex)?;
        }
        PalwConsensusObjectV2::VertexEquivocationV1 { evidence } => {
            palw_vertex_fold_v1::apply_vertex_equivocation_v1(builder, ctx, evidence)?;
        }
        // ---- RFC-0007 Part IV.1: the audit mesh's traps (tags 102, 103) ----
        PalwConsensusObjectV2::TrapCommittedV1 { trap } => {
            palw_mesh_fold_v1::apply_trap_committed_v1(builder, ctx, trap)?;
        }
        PalwConsensusObjectV2::TrapRevealedV1 { reveal } => {
            palw_mesh_fold_v1::apply_trap_revealed_v1(builder, ctx, reveal)?;
        }
        // ---- RFC-0004: the core lane's objects (spec 17 §17.4.2, §17.10) ----
        PalwConsensusObjectV2::ModelLineImprovementPolicySet { payload, signature: _ } => {
            palw_improve_fold_v1::apply_improvement_policy_set_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::LineageHeadRolledBack { payload, filer, signature: _ } => {
            palw_improve_fold_v1::apply_lineage_rollback_v1(builder, ctx, payload, filer)?;
        }
        // ---- RFC-0004: the candidates lane's objects (spec 17 §17.6, §17.7; A4, A5) ----
        PalwConsensusObjectV2::HardCaseSubmitted { payload, submitter, signature: _ } => {
            palw_improve_material_fold_v1::apply_hard_case_v1(builder, ctx, payload, submitter)?;
        }
        PalwConsensusObjectV2::DataUseOptIn { payload, signature: _ } => {
            palw_improve_material_fold_v1::apply_data_use_opt_in_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::SetterSetCommitted { payload, setter, signature: _ } => {
            palw_improve_material_fold_v1::apply_setter_set_committed_v1(builder, ctx, payload, setter)?;
        }
        PalwConsensusObjectV2::SetterSetRevealed { payload } => {
            palw_improve_material_fold_v1::apply_setter_set_revealed_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::SetterKeysRevealed { payload } => {
            palw_improve_material_fold_v1::apply_setter_keys_revealed_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::HardCaseKeyRevealed { payload } => {
            palw_improve_material_fold_v1::apply_case_key_revealed_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::DatasetRegistered { payload, registrant, signature: _ } => {
            palw_improve_material_fold_v1::apply_dataset_registered_v1(builder, ctx, payload, registrant)?;
        }
        PalwConsensusObjectV2::TeachingArtifactCommitted { payload, teacher, signature: _ } => {
            palw_improve_material_fold_v1::apply_artifact_committed_v1(builder, ctx, payload, teacher)?;
        }
        PalwConsensusObjectV2::TeachingArtifactRevealed { payload } => {
            palw_improve_material_fold_v1::apply_artifact_revealed_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::TeacherLicenceRegistered { payload, signature: _ } => {
            palw_improve_material_fold_v1::apply_teacher_licence_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::CandidateSubmitted { payload, submitter, signature: _ } => {
            palw_improve_material_fold_v1::apply_candidate_submitted_v1(builder, ctx, payload, submitter)?;
        }
        PalwConsensusObjectV2::ImprovementPoolFunded { payload } => {
            palw_improve_fold_v1::apply_improvement_pool_funded_v1(builder, ctx, payload)?;
        }
        PalwConsensusObjectV2::BondRegistered {
            bond,
            pubkey,
            operator_pubkey,
            collateral,
            payout_payload,
            capable_classes,
            signature: _,
        } => {
            if builder.state.bonds.contains_key(bond) {
                return Err(PalwStateV2Error::DuplicateBond(*bond));
            }
            // Audit C5: a bond identity has to cost something, or panel dedup is a formality.
            // `min_collateral_sompi` existed in the atomic bundle and was read by nobody.
            // 2026-09-24 DoS audit #12 (c): the registration floor is one function
            // (`palw_bond_registration_floor_v1`) — the producer floor, by the user's decision.
            let floor = palw_bond_registration_floor_v1(builder.params.min_collateral_sompi, builder.extras.audit_2026_09_23_active);
            if *collateral < floor {
                return Err(PalwStateV2Error::CollateralBelowMinimum { bond: *bond, got: *collateral });
            }
            if operator_pubkey.is_empty() {
                return Err(PalwStateV2Error::EmptyOperatorKey(*bond));
            }
            // A zero payload is the P2PKH template over an all-zero address hash — a perfectly
            // well-formed script that nobody holds the preimage to. Every reward this bond ever
            // matured would be minted straight into an unspendable output, and the producer would
            // find out block by block. Registration is where that is knowable, so it is where it
            // is refused.
            // **One key, one bond — the rule this codebase states everywhere and enforced nowhere**
            // (audit M2-19). Panel sortition already refuses a seat sharing the executor's `pubkey`
            // or `operator_id`, which is only meaningful if a key cannot hold several bonds: with
            // no uniqueness check, splitting collateral across N registrations of the SAME key
            // manufactured N independent seat tickets, and a 3-of-5 panel is a permanent quorum for
            // whoever holds three of them.
            if builder.registered_pubkey_v1(pubkey)? {
                return Err(PalwStateV2Error::DuplicateBondKey(*bond));
            }
            // **One identity, one bond** (audit 2026-09-19). The line above makes a second bond
            // cost a second KEY; this one makes it cost a second IDENTITY. `operator_id` is derived
            // from `operator_pubkey`, which no rule in this tree ever verifies a signature under —
            // it is a declaration, and the field's own doc claims the opposite. Undeclared, a
            // registrant could name a victim's identity and be dropped from that victim's panels by
            // `palw_panel_eligible_bonds_v2`'s executor exclusion, taking the victim out of the jury
            // of every claim the attacker produces, for nothing and for ever.
            //
            // Uniqueness does not prove possession — that needs a signature under the operator key,
            // which changes the object's bytes and so waits for a wire break. It does make the
            // attack a race against a registration rather than a free primitive: an identity
            // already on the chain cannot be claimed a second time.
            if builder.extras.operator_id_unique_active {
                let operator = palw_operator_id_v2(operator_pubkey);
                if let Some(holder) = builder.registered_operator_v1(operator)? {
                    return Err(PalwStateV2Error::DuplicateOperator { operator, holder });
                }
            }
            if *payout_payload == Hash64::default() {
                return Err(PalwStateV2Error::EmptyPayoutPayload(*bond));
            }
            // **ADR-0071 SA-1/SA-2/SA-4 at the OTHER writer of `capable_classes`** (mainnet audit,
            // 2026-09-05). Every amendment guarded `BondCapabilityDeclared` and none guarded this
            // arm, which writes the registrant's set straight into `state_root`: unknown ids, an
            // unbounded count, and a reservation the collateral cannot carry, all admitted at
            // registration and refused only at the next declaration. One rule, two writers — the
            // shape this codebase names as the one two readers change apart. Under the same fence,
            // for the same reason, and by the same function. (Naming a class not yet registered
            // is allowed HERE and refused at declaration: a registrant's bond declares for the class
            // it is about to register, and that registration's carriage needs the bond first.)
            if builder.capability_bound {
                check_capable_classes_declared_v1(&builder.state, *bond, capable_classes, false, 0, *collateral)?;
            }
            // The constructor clones these three typed fields. Reserve that
            // input copy before it creates public-key/capability allocations.
            {
                use crate::palw_native_resource_v1::NativeHeapV1;
                let bound = (pubkey, operator_pubkey, capable_classes).native_heap_v1("native registration input copy");
                builder
                    .native_reservation
                    .grow_v1(bound.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(bound))?)
                    .map_err(PalwStateV2Error::NativeResource)?;
            }
            builder.write_bond(
                *bond,
                Some(palw_bond_state_from_registration_v2(
                    pubkey,
                    operator_pubkey,
                    *collateral,
                    *payout_payload,
                    ctx.daa_score,
                    capable_classes.clone(),
                )),
            )?;
        }
        PalwConsensusObjectV2::ModelBuy { line_id, holder, msk_in, min_units_out, sink_index: _ } => {
            model_buy_v1(builder, ctx, line_id, holder, *msk_in, *min_units_out)?;
        }
        PalwConsensusObjectV2::ModelSell { line_id, holder, units_in, min_msk_out, held_units, pubkey, .. } => {
            // **Past the 2026-09-23 fence the net leg is paid where the seller's key can spend it**
            // (the 2026-09-23 Position route matrix, P-B4). It used to be paid to `holder`, the
            // key's unkeyed id, and the coinbase locks a payout with `p2pkh_mldsa87_spk(payload)`,
            // whose `OP_BLAKE2B_512` recomputes the KEYED address payload at spend time: the id
            // never matches, so every sell's proceeds were an output no key could spend. The
            // position stays keyed by `holder`; only the payee changes. Below the fence the old
            // payee stands byte for byte, because testnet-11 armed the market before this fence.
            let net_payload = if builder.extras.audit_2026_09_23_active {
                if crate::palw_model_market_v1::palw_model_holder_of_pubkey_v1(pubkey) != *holder {
                    return Err(PalwStateV2Error::ModelSellKeyIsNotTheHolders(*line_id));
                }
                crate::palw_model_market_v1::palw_model_sell_net_payload_v1(pubkey)
            } else {
                *holder
            };
            model_sell_v1(builder, ctx, line_id, holder, *units_in, *min_msk_out, Some(net_payload), Some(*held_units))?;
        }
        // **ADR-0099 Decision 5, built by ADR-0100: the one-move court.** Fence first; then the
        // claim (live, its executor and roots the object's); then the accuser (not the producer,
        // Active, at or above the floor — the DA court's terms, because an accusation is priced,
        // not privileged); then one court at a time on a claim; then the verdict, derived HERE at
        // the ladder the block's extras carry, and applied: a fault voids the claim and slashes
        // `claim.reserved` (Decision 8's `CourtFraud`, never the whole bond), a refutation that
        // proves none charges the accuser what it staked. A fused site is never tried in one move:
        // under the held regime it opens a held dissection at the named leaf (ADR-0103 Decision 5);
        // below it, and past `palw_offence_attribution` for a held class no honest party can dissect
        // inside a turn (ADR-0152 §4-ter), it is refused — the acceptance layer refuses it first,
        // and this arm is the second lock on that door.
        // **RFC-0002 Phase F (tag 62): an IR claim accused in one move.** The legacy one-move court's
        // gates in its order — a live claim, the executor and the roots the claim's, an accuser that
        // is not the producer, the open-session rule, an Active accuser at or above the floor — on a
        // claim of an IR class; then the verdict the acceptance layer re-derived from the proof
        // (`palw_tir_one_move_v1`), applied as the legacy court applies its own.
        PalwConsensusObjectV2::TirShardCourtAccused { accusation } => {
            let claim_id = accusation.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "TirShardCourtAccused" });
            }
            if !builder.state.tir_classes.contains_key(&claim.class_id) {
                return Err(PalwStateV2Error::TirRegistrationRefused(
                    "an IR one-move accusation names a claim of a class that is not an IR program",
                ));
            }
            // RFC-0004 A6: an evaluation proof accuses an evaluation claim of the job table, and no other.
            if accusation.proof.is_eval_v1() && !palw_improve_eval_fold_v1::palw_improve_claim_is_evaluation_v1(&claim) {
                return Err(PalwStateV2Error::ImprovementObjectRefused {
                    object: "an evaluation court move",
                    why: "it accuses a claim that is not an evaluation claim",
                });
            }
            if accusation.executor_bond != claim.bond {
                return Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(claim_id));
            }
            if accusation.execution_root != claim.execution_root || accusation.trace_root != claim.trace_root {
                return Err(PalwStateV2Error::ShardCourtRootsDiffer(claim_id));
            }
            let accuser = accusation.accuser_bond;
            if accuser == claim.bond {
                return Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(accuser));
            }
            let open_session = builder.state.court_sessions.iter().find(|(_, s)| s.claim == claim_id).map(|(id, _)| *id);
            if let Some(session) = open_session
                && !builder.extras.offence_attribution_active
            {
                return Err(PalwStateV2Error::ShardCourtClaimUnderSession { claim: claim_id, session });
            }
            let accuser_record = builder.state.bonds.get(&accuser).ok_or(PalwStateV2Error::MissingBond(accuser))?.clone();
            if !matches!(accuser_record.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(accuser));
            }
            let floor = builder.params.min_collateral_sompi();
            if accuser_record.collateral < floor {
                return Err(PalwStateV2Error::BondBelowFloor { bond: accuser, collateral: accuser_record.collateral, floor });
            }
            // **RFC-0002 F7 (ADR-0103 Decision 5 for an IR program): under the held regime a cone
            // accusation at a DISSECTED leaf is the challenge.** Its cone reduces over the history, so
            // it is never tried in one move there: the leaf is named, the session opens at `Terminal`
            // on it, and the responder's IR root claim is its first clocked move — spec 04b §9.5 from
            // then on. The leaf is bound to the claim first (the binding the claim's, the leaf opened
            // under it); the acceptance layer refused any other declared verdict, and this is the
            // second lock on that door. Outside the held regime the bisection court reaches the leaf,
            // and a cone accusation is adjudicated whole as every IR close is.
            let dissected_leaf = match builder.extras.held_context_ladder {
                Some(held) => crate::palw_tir_one_move_v1::palw_tir_one_move_dissected_leaf_v1(&builder.state, &claim, accusation)
                    .map_err(|e| PalwStateV2Error::ShardCourt(e.to_string()))?
                    .map(|leaf| (held, leaf)),
                None => None,
            };
            if let Some((held, leaf)) = dissected_leaf {
                if accusation.verdict != PalwCourtVerdictV2::ExecutorGuilty {
                    return Err(PalwStateV2Error::ShardCourt("an accusation that opens a dissection declares ExecutorGuilty".into()));
                }
                if builder.extras.offence_attribution_active && builder.params.held_class_is_unanswerable_v1(&claim.class_id) {
                    return Err(PalwStateV2Error::ShardCourtHeldSiteUnanswerable { claim: claim_id, leaf, class: claim.class_id });
                }
                if open_session.is_some() {
                    check_further_held_dissection_v1(&builder.state, claim_id, accuser)?;
                }
                let ladder = builder.state.class_step_ladder_v1(&claim.class_id, held);
                let turn = builder.params.turn_deadline_daa();
                // D2 (REQ-T6-4): the cone accusation's params are what it publishes for the root claim's finalize to relay (read
                // only on a new claim).
                let published = match &accusation.proof {
                    crate::palw_court_v2::PalwCourtVerdictProofV2::TirCone { refutation } => {
                        crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::TirParams(&refutation.params)
                    }
                    crate::palw_court_v2::PalwCourtVerdictProofV2::EvalCone { .. } => {
                        crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::CarriedUnchecked
                    }
                    _ => crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::None,
                };
                open_dissection_at_named_leaf_v1(builder, ctx, claim_id, &claim, accuser, leaf, ladder, turn, published)?;
            } else {
                match accusation.verdict {
                    PalwCourtVerdictV2::ExecutorGuilty => builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, accuser, None)?,
                    PalwCourtVerdictV2::ChallengerDefeated => {
                        let charge = crate::palw_shard_court_v1::palw_shard_court_false_accusation_charge_v1(claim.reserved, floor);
                        builder.slash_seat(accuser, charge, floor)?;
                    }
                }
            }
        }
        // **RFC-0003 §I.4.7: a pipeline claim accused in one move.** The IR arm above with the class check
        // turned to the generative registry: a live claim of a generative class, the executor and the roots
        // the claim's, an accuser that is not the producer, the open-session rule, an Active accuser at or
        // above the floor — then the verdict the acceptance layer re-derived from the proof
        // (`palw_gen_one_move_v1`), applied as the legacy court applies its own. Under the held regime a cone
        // accusation at a DISSECTED leaf opens a dissection there (the responder's generative root claim is
        // its first move); below `palw_gen_v1` the lock above refused the object by name.
        PalwConsensusObjectV2::GenShardCourtAccused { accusation } => {
            let claim_id = accusation.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "GenShardCourtAccused" });
            }
            if !builder.state.gen_classes.contains_key(&claim.class_id) {
                return Err(PalwStateV2Error::GenObjectRefused(
                    "a generative one-move accusation names a claim of a class that is not a pipeline",
                ));
            }
            if accusation.executor_bond != claim.bond {
                return Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(claim_id));
            }
            if accusation.execution_root != claim.execution_root || accusation.trace_root != claim.trace_root {
                return Err(PalwStateV2Error::ShardCourtRootsDiffer(claim_id));
            }
            let accuser = accusation.accuser_bond;
            if accuser == claim.bond {
                return Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(accuser));
            }
            let open_session = builder.state.court_sessions.iter().find(|(_, s)| s.claim == claim_id).map(|(id, _)| *id);
            if let Some(session) = open_session
                && !builder.extras.offence_attribution_active
            {
                return Err(PalwStateV2Error::ShardCourtClaimUnderSession { claim: claim_id, session });
            }
            let accuser_record = builder.state.bonds.get(&accuser).ok_or(PalwStateV2Error::MissingBond(accuser))?.clone();
            if !matches!(accuser_record.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(accuser));
            }
            let floor = builder.params.min_collateral_sompi();
            if accuser_record.collateral < floor {
                return Err(PalwStateV2Error::BondBelowFloor { bond: accuser, collateral: accuser_record.collateral, floor });
            }
            let dissected_leaf = match builder.extras.held_context_ladder {
                Some(held) => crate::palw_gen_one_move_v1::palw_gen_one_move_dissected_leaf_v1(&builder.state, &claim, accusation)
                    .map_err(|e| PalwStateV2Error::ShardCourt(e.to_string()))?
                    .map(|leaf| (held, leaf)),
                None => None,
            };
            if let Some((held, leaf)) = dissected_leaf {
                if accusation.verdict != PalwCourtVerdictV2::ExecutorGuilty {
                    return Err(PalwStateV2Error::ShardCourt("an accusation that opens a dissection declares ExecutorGuilty".into()));
                }
                if open_session.is_some() {
                    check_further_held_dissection_v1(&builder.state, claim_id, accuser)?;
                }
                let ladder = builder.state.class_step_ladder_v1(&claim.class_id, held);
                let turn = builder.params.turn_deadline_daa();
                // D2 (REQ-T6-4): the cone accusation's params are what it publishes for the root claim's finalize to relay (read
                // only on a new claim).
                let published = match &accusation.proof {
                    crate::palw_court_v2::PalwCourtVerdictProofV2::GenCone { close } => {
                        crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::GenParams(&close.params)
                    }
                    _ => crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::None,
                };
                open_dissection_at_named_leaf_v1(builder, ctx, claim_id, &claim, accuser, leaf, ladder, turn, published)?;
            } else {
                match accusation.verdict {
                    PalwCourtVerdictV2::ExecutorGuilty => builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, accuser, None)?,
                    PalwCourtVerdictV2::ChallengerDefeated => {
                        let charge = crate::palw_shard_court_v1::palw_shard_court_false_accusation_charge_v1(claim.reserved, floor);
                        builder.slash_seat(accuser, charge, floor)?;
                    }
                }
            }
        }
        // **RFC-0003 decision 22: a named-leaf challenge with the declaration of its close** — the session at
        // `Terminal` on the leaf and the challenger-side group, in one step (`palw_held_close_fold_v1`).
        PalwConsensusObjectV2::HeldLeafChallengeDeclared { challenge } => {
            palw_held_close_fold_v1::apply_held_leaf_challenge_v1(builder, ctx, challenge)?;
        }
        // **RFC-0001 §2.10: an adapter class's listing** — the registry entry a composite opening and a seat's
        // possession proof read (`palw_adapter_class_fold_v1::apply_adapter_class_listed_v1`).
        PalwConsensusObjectV2::AdapterClassListed { payload, lister, signature: _ } => {
            palw_adapter_class_fold_v1::apply_adapter_class_listed_v1(builder, ctx, payload, lister)?;
        }
        // **G14 lane D (tags 110 and 111): the kernel route** — dormant behind `palw_probabilistic_constraints_v1`, which no network
        // can arm (`palw_kernel_route_fold_v1`). A kernel refusal drops the object; only a missing fence fails the block.
        PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature: _ } => {
            palw_kernel_route_fold_v1::apply_kernel_route_object_v1(builder, ctx, bytes, signer)?;
        }
        PalwConsensusObjectV2::KernelConstraintReceiptV1 { receipt, signature } => {
            palw_kernel_route_fold_v1::apply_kernel_receipt_v1(builder, ctx, receipt, signature)?;
        }
        // (tag 113, C4 F-C4R3-03): a chunk of a prosecution object in the route's own chunk lane.
        PalwConsensusObjectV2::KernelRouteChunkV1 { chunk, signature: _ } => {
            palw_kernel_route_fold_v1::apply_kernel_route_chunk_v1(builder, ctx, chunk)?;
        }
        // **G14 lane D phase 3 (tags 104–107): model onboarding** — rows in the route's aux tables (`palw_onboarding_fold_v1`).
        PalwConsensusObjectV2::ArtifactBoundV1 { v2_class, kernel_param_root, signer, signature: _ } => {
            palw_onboarding_fold_v1::apply_artifact_bound_v1(builder, ctx, v2_class, kernel_param_root, signer)?;
        }
        PalwConsensusObjectV2::ArtifactBindingChallengedV1 { v2_class, kernel_param_root, challenger, proof, signature: _ } => {
            palw_onboarding_fold_v1::apply_artifact_challenged_v1(builder, ctx, v2_class, kernel_param_root, challenger, proof)?;
        }
        PalwConsensusObjectV2::KernelBoundV1 { v2_class, kernel_class, challenge_policy_id, signer, signature: _ } => {
            palw_onboarding_fold_v1::apply_kernel_bound_v1(builder, ctx, v2_class, kernel_class, challenge_policy_id, signer)?;
        }
        PalwConsensusObjectV2::ConformanceCommittedV1 { commitment, signer, signature: _ } => {
            palw_onboarding_fold_v1::apply_conformance_committed_v1(builder, ctx, commitment, signer)?;
        }
        // **Onboarding P0 (tag 109): conformance evidence** — posted or refuted; tables 39 and 40 (`palw_onboarding_fold_v1`).
        PalwConsensusObjectV2::ConformanceEvidenceV1 { v2_class, action, signer, signature: _ } => {
            palw_onboarding_fold_v1::apply_conformance_evidence_v1(builder, ctx, v2_class, action, signer)?;
        }
        // **Lane DA16 (tags 150–153): the provider court** — rows in the route's aux tables 43–45 (`palw_provider_court_fold_v1`).
        PalwConsensusObjectV2::MonetaryMaterialEnrolledV1 { enrollment, signature: _ } => {
            palw_monetary_material_fold_v1::enroll(builder, ctx, enrollment)?;
        }
        PalwConsensusObjectV2::MonetaryMaterialReplacedV1 { replacement, signature: _ } => {
            palw_monetary_material_fold_v1::replace(builder, ctx, replacement)?;
        }
        PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, signer, signature: _ } => {
            palw_monetary_material_fold_v1::action(builder, ctx, action, signer)?;
        }
        PalwConsensusObjectV2::ProviderLeaseV1 { subject, reserved, serve_until_daa, provider, signature: _ } => {
            palw_provider_court_fold_v1::apply_provider_lease_v1(builder, ctx, subject, *reserved, *serve_until_daa, provider)?;
        }
        PalwConsensusObjectV2::ProviderChallengeV1 { subject, provider, unit, valid_until_daa, challenger, signature: _ } => {
            palw_provider_court_fold_v1::apply_provider_challenge_v1(
                builder,
                ctx,
                subject,
                provider,
                unit,
                *valid_until_daa,
                challenger,
            )?;
        }
        PalwConsensusObjectV2::ProviderAnswerV1 { subject, provider, unit, answer, signature: _ } => {
            palw_provider_court_fold_v1::apply_provider_answer_v1(builder, ctx, subject, provider, unit, answer)?;
        }
        PalwConsensusObjectV2::DaTransferV1 { claim, producer, signature: _ } => {
            palw_provider_court_fold_v1::apply_da_transfer_v1(builder, ctx, claim, producer)?;
        }
        // Lane LG14-A (tags 154–155): the legacy route's dispute reservation. Refused below `palw_legacy_public_filer_v1` (the
        // acceptance walk drops them by name first; this is the second lock).
        PalwConsensusObjectV2::DisputeReservedV1 { reservation, signature: _ } => {
            palw_legacy_public_filer_fold_v1::apply_dispute_reserved_v1(builder, ctx, reservation)?;
        }
        PalwConsensusObjectV2::DisputeReacquiredV1 { request, signature: _ } => {
            palw_legacy_public_filer_fold_v1::apply_dispute_reacquired_v1(builder, ctx, request)?;
        }
        PalwConsensusObjectV2::DisputeReleasedV1 { claim, reserver, signature: _ } => {
            palw_legacy_public_filer_fold_v1::apply_dispute_released_v1(builder, ctx, *claim, *reserver)?;
        }
        // **Lane LG14-B (tags 157–159): the legacy V2 route's public descent, its held DA units and the leaf recompute** — dormant
        // behind `palw_legacy_held_da_v2` (each arm refuses below it: the second lock; `crate::palw_legacy_held_da_v2`).
        PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand } => {
            open_da_session_legacy_held_v2(builder, ctx, demand)?;
        }
        PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer } => {
            apply_legacy_held_answer_v2(builder, ctx, answer)?;
            palw_legacy_answer_chunks_fold_v2::sweep(builder, ctx)?;
        }
        PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 { declaration } => {
            palw_legacy_answer_chunks_fold_v2::declare(builder, ctx, declaration)?;
        }
        PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk } => {
            palw_legacy_answer_chunks_fold_v2::chunk(builder, ctx, chunk)?;
        }
        PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed } => palw_staged_court_fold_v1::open(builder, ctx, seed)?,
        PalwConsensusObjectV2::PublicCourtStreamLeafV1 { leaf } => palw_staged_court_fold_v1::leaf(builder, ctx, leaf)?,
        PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } => palw_staged_court_fold_v1::movement(builder, ctx, movement)?,
        PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { seed } => palw_staged_court_fold_v1::open_native(builder, ctx, seed)?,
        PalwConsensusObjectV2::TirClassLayoutPublishedV1 { class_id, layout } => {
            palw_staged_court_fold_v1::publish_tir_layout(builder, ctx, *class_id, layout)?
        }
        PalwConsensusObjectV2::ObjectiveEvidenceDeclaredV1 { declaration } => {
            palw_terminal_carriage_fold_v1::declare(builder, ctx, declaration)?;
        }
        PalwConsensusObjectV2::ObjectiveEvidenceChunkV1 { part } => {
            palw_terminal_carriage_fold_v1::part(builder, ctx, part)?;
        }
        PalwConsensusObjectV2::LegacyLeafRecomputedV2 { accusation } => {
            apply_legacy_leaf_recompute_v2(builder, ctx, accusation)?;
        }
        // **Lane BUDGET (tag 140): a bond's capital assignment** — table 4 of the bond budget (`palw_bond_budget_fold_v1`).
        PalwConsensusObjectV2::BondCapitalAssignedV1 { bond, assignments, sequence, signature: _ } => {
            palw_bond_budget_fold_v1::apply_capital_assignment_v1(builder, ctx, bond, assignments, *sequence)?;
        }
        // (tag 108): the acceptance walk replaces the envelope by the registration it wraps, so the fold never meets one.
        PalwConsensusObjectV2::SignedRegistrationV1 { .. } => {
            return Err(PalwStateV2Error::KernelRouteRefused(
                "a signed registration envelope is unwrapped by the acceptance walk; the fold folds the registration it wraps"
                    .to_string(),
            ));
        }
        PalwConsensusObjectV2::GdnStepAccusedV2 { accusation } => {
            let limits = builder.extras.memory_routing_court.ok_or(PalwStateV2Error::ShardCourtDormant)?;
            let claim_id = accusation.claim;
            let source = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
            let _claim_copy = crate::palw_native_resource_v1::reserve_native_requests_v1(
                crate::palw_native_resource_v1::native_value_requests_v1(source).map_err(PalwStateV2Error::NativeResource)?,
            )
            .map_err(PalwStateV2Error::NativeResource)?;
            let claim = source.clone();
            let ladder = builder.state.class_step_ladder_v1(&claim.class_id, limits.ladder);
            accusation.validate_shape(ladder).map_err(|e| PalwStateV2Error::ShardCourt(e.into()))?;
            if accusation.encoded_bytes().map_err(|e| PalwStateV2Error::ShardCourt(e.into()))? > limits.max_close_bytes {
                return Err(PalwStateV2Error::ShardCourt("GDN accusation exceeds the current court close ceiling".into()));
            }
            let reserved_final = matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
                && builder.state.legacy_dispute_reservation_v1(&claim_id, &accusation.accuser_bond).is_some();
            if claim.phase.is_terminal() && !reserved_final {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "GdnStepAccusedV2" });
            }
            if accusation.executor_bond != claim.bond {
                return Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(claim_id));
            }
            if accusation.execution_root != claim.execution_root || accusation.trace_root != claim.trace_root {
                return Err(PalwStateV2Error::ShardCourtRootsDiffer(claim_id));
            }
            let accuser = accusation.accuser_bond;
            if accuser == claim.bond {
                return Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(accuser));
            }
            if !builder.extras.offence_attribution_active
                && let Some((session, _)) = builder.state.court_sessions.iter().find(|(_, row)| row.claim == claim_id)
            {
                return Err(PalwStateV2Error::ShardCourtClaimUnderSession { claim: claim_id, session: *session });
            }
            let record = builder.state.bonds.get(&accuser).ok_or(PalwStateV2Error::MissingBond(accuser))?;
            if !matches!(record.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(accuser));
            }
            let floor = builder.params.min_collateral_sompi();
            if record.collateral < floor {
                return Err(PalwStateV2Error::BondBelowFloor { bond: accuser, collateral: record.collateral, floor });
            }
            let artifact_root =
                builder.state.classes.get(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.artifact_root;
            let actual = crate::palw_shard_court_v1::PalwOneMoveClaimV2 {
                execution_root: claim.execution_root,
                class_id: claim.class_id,
                artifact_root,
            };
            let verdict = crate::palw_gdn_court_v2::palw_gdn_court_verdict_v2(accusation, &actual, ladder).map_err(|e| match e {
                crate::palw_step_refute::memory_v2::PalwGdnCourtErrorV2::Resource(cause) => PalwStateV2Error::NativeResource(cause),
                other => PalwStateV2Error::ShardCourt(other.to_string()),
            })?;
            match verdict {
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty => {
                    builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, accuser, None)?
                }
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::FalseAccusation => {
                    let charge = crate::palw_shard_court_v1::palw_shard_court_false_accusation_charge_v1(claim.reserved, floor);
                    builder.slash_seat(accuser, charge, floor)?;
                }
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection => {
                    return Err(PalwStateV2Error::ShardCourt("GDN terminal must never invent an attention dissection".into()));
                }
            }
        }
        // RFC-0006 consumer v2 (tag 244): the router binding and the checkpoint / routed accusations.
        PalwConsensusObjectV2::MemoryRoutingV2 { object } => {
            super::palw_memory_routing_fold_v2::apply_memory_routing_v2(builder, ctx, object)?;
        }
        PalwConsensusObjectV2::ShardCourtAccused { accusation } => {
            let Some(ladder) = builder.extras.shard_court_ladder else {
                return Err(PalwStateV2Error::ShardCourtDormant);
            };
            let claim_id = accusation.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            // ADR-0119 Decision 4: the CLAIM's ladder — a held class's recorded one, else the network's.
            let ladder = builder.state.class_step_ladder_v1(&claim.class_id, ladder);
            // Lane LG14-A × LG14-B (RFC-0014 §7.3): a `Final` claim the accuser's own live dispute reservation holds is still pursued —
            // the fused terminal after `Final` (the held dissection's one move). The table is empty below the fence: unchanged there.
            let reserved_final = matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
                && builder.state.legacy_dispute_reservation_v1(&claim_id, &accusation.accuser_bond).is_some();
            if claim.phase.is_terminal() && !reserved_final {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "ShardCourtAccused" });
            }
            if accusation.executor_bond != claim.bond {
                return Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(claim_id));
            }
            if accusation.execution_root != claim.execution_root || accusation.trace_root != claim.trace_root {
                return Err(PalwStateV2Error::ShardCourtRootsDiffer(claim_id));
            }
            let accuser = accusation.accuser_bond;
            if accuser == claim.bond {
                return Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(accuser));
            }
            // **ADR-0152 §4-ter C3: one court at a time is no shield past the fence.** Below it any
            // open session refuses the accusation. Past `palw_offence_attribution` a one-move
            // verdict (guilty or false) lands whatever is open — a guilty one voids the claim and
            // every session on it closes neutrally (`void_claim`) — and only a further DISSECTION is
            // gated, once the verdict says the leaf needs one (below).
            let open_session = builder.state.court_sessions.iter().find(|(_, s)| s.claim == claim_id).map(|(id, _)| *id);
            if let Some(session) = open_session
                && !builder.extras.offence_attribution_active
            {
                return Err(PalwStateV2Error::ShardCourtClaimUnderSession { claim: claim_id, session });
            }
            let accuser_record = builder.state.bonds.get(&accuser).ok_or(PalwStateV2Error::MissingBond(accuser))?.clone();
            if !matches!(accuser_record.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(accuser));
            }
            let floor = builder.params.min_collateral_sompi();
            if accuser_record.collateral < floor {
                return Err(PalwStateV2Error::BondBelowFloor { bond: accuser, collateral: accuser_record.collateral, floor });
            }
            let artifact_root =
                builder.state.classes.get(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.artifact_root;
            // t12 (`palw_audit_2026_09_23`): the verdict is BOUND to the claim before a fused site is
            // deferred — the binding recomputes to the claim's root, the named tile opens under it,
            // the rows prove against the class — so a court opens only on evidence of THIS claim.
            // Every refusal is a refusal (the object is not folded); nothing here charges the
            // accuser, whose signature does not cover the refutation's bytes.
            let bound_to = crate::palw_shard_court_v1::PalwOneMoveClaimV2 {
                execution_root: claim.execution_root,
                class_id: claim.class_id,
                artifact_root,
            };
            let verdict = crate::palw_shard_court_v1::palw_shard_court_verdict_at_v2(
                accusation,
                &bound_to,
                ladder,
                builder.extras.audit_2026_09_23_active,
            )
            .map_err(|e| PalwStateV2Error::ShardCourt(e.to_string()))?;
            match verdict {
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty => {
                    // A proven verdict (S-4): `CourtConviction` past `palw_rcore_plus`, the accuser its
                    // challenger; below it, the void as it always was.
                    builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, accuser, None)?;
                }
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::FalseAccusation => {
                    let charge = crate::palw_shard_court_v1::palw_shard_court_false_accusation_charge_v1(claim.reserved, floor);
                    builder.slash_seat(accuser, charge, floor)?;
                }
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection => {
                    // **ADR-0103 Decision 5: under the held regime the accusation IS the challenge.**
                    // The leaf is named, so the ladder a bisection would have walked to reach it is
                    // not played: the session opens at `Terminal` on that leaf and the responder's
                    // root claim is its first clocked move — ADR-0082's dissection from there on,
                    // with every rule of it unchanged. Dormant, it is the refusal it always was.
                    if builder.extras.held_context_ladder.is_none() {
                        return Err(PalwStateV2Error::ShardCourtNeedsDissection { claim: claim_id, leaf: accusation.leaf_index });
                    }
                    // **ADR-0152 §4-ter (A-held): a dissection nobody can play is not opened.** Past
                    // `palw_offence_attribution` a held class above `PALW_HELD_ANSWERABLE_N_CTX_V1`
                    // (testnet-12's 2M row) has no honest move inside a turn on either side, so its
                    // session could only freeze the claim's `Final` for a window and end in the
                    // mercy: the accusation is refused, and the class's residual is (C)'s bound (held
                    // to Final, `c_2M = 1`, RT#2). The answerable held class (8k) opens as before.
                    if builder.extras.offence_attribution_active && builder.params.held_class_is_unanswerable_v1(&claim.class_id) {
                        return Err(PalwStateV2Error::ShardCourtHeldSiteUnanswerable {
                            claim: claim_id,
                            leaf: accusation.leaf_index,
                            class: claim.class_id,
                        });
                    }
                    // **ADR-0152 §4-ter C3: a session already open admits one more dissection only
                    // from a seat of the claim's panel** — one per seat, at most `1 + seat_count` on
                    // the claim, AT ANY LEAF: a decoy the producer's own Sybil opened — at an honest
                    // leaf, or at the lie's own leaf to hold that leaf — holds off no seat (the
                    // review's F2: a same-leaf refusal let one decoy at the lie lock out the whole
                    // panel). Two sessions at one leaf do not collide: the session id binds the
                    // challenger (`open_at_named_leaf`). A non-seat's dissection waits, as one court
                    // at a time always made it.
                    if open_session.is_some() {
                        check_further_held_dissection_v1(&builder.state, claim_id, accuser)?;
                    }
                    open_held_dissection_v1(builder, ctx, claim_id, &claim, accusation, ladder)?;
                }
            }
        }
        // **ADR-0103 Decision 1: a checkpoint's chunk against the rows it claims to hold, in one
        // move.** The shard court's gates in the shard court's order — the fence, a live claim, the
        // executor and the roots the claim's, an accuser that is not the producer, no session open
        // on the claim, an Active bond at or above the floor — then the verdict, derived here at the
        // ladder the extras carry, exactly as the acceptance layer derived it.
        PalwConsensusObjectV2::CheckpointAccused { accusation } => {
            let Some(ladder) = builder.extras.held_context_ladder else {
                return Err(PalwStateV2Error::HeldContextDormant);
            };
            let claim_id = accusation.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            // ADR-0119 Decision 4: the CLAIM's ladder.
            let ladder = builder.state.class_step_ladder_v1(&claim.class_id, ladder);
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "CheckpointAccused" });
            }
            if accusation.executor_bond != claim.bond {
                return Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(claim_id));
            }
            if accusation.execution_root != claim.execution_root || accusation.trace_root != claim.trace_root {
                return Err(PalwStateV2Error::ShardCourtRootsDiffer(claim_id));
            }
            let accuser = accusation.accuser_bond;
            if accuser == claim.bond {
                return Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(accuser));
            }
            // ADR-0152 §4-ter C3: past the fence a one-move checkpoint verdict lands whatever court
            // is open on the claim (a guilty one voids it, and its sessions close neutrally).
            if let Some((session, _)) = builder.state.court_sessions.iter().find(|(_, s)| s.claim == claim_id)
                && !builder.extras.offence_attribution_active
            {
                return Err(PalwStateV2Error::ShardCourtClaimUnderSession { claim: claim_id, session: *session });
            }
            let accuser_record = builder.state.bonds.get(&accuser).ok_or(PalwStateV2Error::MissingBond(accuser))?.clone();
            if !matches!(accuser_record.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(accuser));
            }
            let floor = builder.params.min_collateral_sompi();
            if accuser_record.collateral < floor {
                return Err(PalwStateV2Error::BondBelowFloor { bond: accuser, collateral: accuser_record.collateral, floor });
            }
            let verdict = crate::palw_checkpoint_court_v1::palw_checkpoint_court_verdict_v1(accusation, claim.class_id, ladder)
                .map_err(|e| PalwStateV2Error::CheckpointCourt(e.to_string()))?;
            match verdict {
                crate::palw_checkpoint_court_v1::PalwCheckpointCourtVerdictV1::ExecutorGuilty => {
                    // ADR-0152 §4-ter.3 step 6 (the forger's race): the acquittals this proves wrong,
                    // read before the conviction's void drops them.
                    let wronged = builder.held_forfeits_proven_wrong_v1(&claim_id, accusation);
                    // A proven verdict (S-4): `CourtConviction` past `palw_rcore_plus`.
                    builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, accuser, None)?;
                    // ...and, the conviction closed, each wronged challenger made whole.
                    builder.refund_held_forfeits_v1(&wronged)?;
                }
                crate::palw_checkpoint_court_v1::PalwCheckpointCourtVerdictV1::FalseAccusation => {
                    let charge = crate::palw_shard_court_v1::palw_shard_court_false_accusation_charge_v1(claim.reserved, floor);
                    builder.slash_seat(accuser, charge, floor)?;
                }
            }
        }
        // **ADR-0103 Decision 4: a held accusation opens ADR-0062's session over a named unit.**
        // The unit is bounded against the claim's own binding first — an accusation of a unit the
        // claim never committed is one no producer could answer — and then every gate, clock,
        // reservation and sweep of the event court applies unchanged; the phase carries the
        // sentinel in place of an event index and the unit is recorded beside it.
        PalwConsensusObjectV2::DefaultAccusedHeld { accusation } => {
            if builder.extras.held_context_ladder.is_none() {
                return Err(PalwStateV2Error::HeldContextDormant);
            }
            // ADR-0152 DA-1 (M3): past `palw_rcore_plus` a held accusation opens a session in the side
            // maps, with drawn units, a free named leaf and no once-per-seat rule.
            if builder.params.rcore_plus_active_at(ctx.daa_score) {
                return open_da_session_rcore_v1(
                    builder,
                    ctx,
                    accusation.claim,
                    accusation.accuser,
                    crate::palw_da_rcore_v1::PalwDaUnitV1::Held(accusation.missing),
                    Some(&accusation.binding),
                );
            }
            let claim_id = accusation.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            crate::palw_held_da_v1::palw_held_da_check_accusation_v1(
                &claim.execution_root,
                &accusation.missing,
                &accusation.binding,
                builder.extras.prompt_ids_form_v1(),
            )
            .map_err(|e| PalwStateV2Error::HeldDaRefused { claim: claim_id, why: e.to_string() })?;
            // F1 (SPEC §4.6): an accusation whose binding fails the claim's identity is not about
            // this claim's job — its filer proves that with `ExecutorRefuted`, not with a session.
            if let Some(why) = builder.da_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &accusation.binding) {
                return Err(PalwStateV2Error::HeldDaRefused { claim: claim_id, why });
            }
            // **ADR-0111 Decision 3: a leaf's evidence is demanded by a seat, once.** The seat of
            // the claim's bound panel, never a stranger — whose leaf is one its own draw assigned
            // it is the acceptance layer's check, which holds the network domain the draw is keyed
            // by — and never twice on one claim, so what the executor owes is bounded by the panel.
            let demand = match accusation.missing {
                crate::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf } => {
                    let refused = |why: &str| PalwStateV2Error::HeldDaRefused { claim: claim_id, why: why.to_string() };
                    let seated = builder
                        .state
                        .panels
                        .get(&claim_id)
                        .is_some_and(|panel| panel.seats.iter().any(|seat| seat.bond == accusation.accuser));
                    if !seated {
                        return Err(refused("a leaf's evidence is demanded by a seat of the claim's panel"));
                    }
                    let mut demands = builder.state.held_leaf_demands.get(&claim_id).cloned().unwrap_or_default();
                    if demands.contains_key(&accusation.accuser) {
                        return Err(refused("this seat has demanded a leaf of this claim before"));
                    }
                    demands.insert(accusation.accuser, leaf);
                    Some(demands)
                }
                _ => None,
            };
            open_da_session_v2(
                builder,
                ctx,
                claim_id,
                crate::palw_held_da_v1::PALW_HELD_DA_EVENT_INDEX_SENTINEL_V1,
                &accusation.accuser,
                false,
            )?;
            builder.write_held_da_missing(claim_id, Some(accusation.missing))?;
            if let Some(demands) = demand {
                builder.write_held_leaf_demands(claim_id, Some(demands))?;
            }
        }
        // **ADR-0103 Decision 4: the answer, and it is arithmetic.** The unit must be the one
        // recorded; the answer is checked against the claim's committed roots through its binding;
        // then the event court's refutation arm runs verbatim — the accuser alone pays, the pause is
        // given back, the claim's clock re-arms.
        PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } => {
            let Some(ladder) = builder.extras.held_context_ladder else {
                return Err(PalwStateV2Error::HeldContextDormant);
            };
            if builder.params.rcore_plus_active_at(ctx.daa_score) {
                return Err(PalwStateV2Error::DaV1AnswerRetired("MaterialDisclosedHeld"));
            }
            if !builder.da_court {
                return Err(PalwStateV2Error::DaCourtDormant);
            }
            let claim_id = disclosure.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            // ADR-0119 Decision 4: the answer is opened at the CLAIM's ladder, the one its demand
            // was bounded against — else an honest producer's answer past the network's ladder would
            // be refused and the producer slashed for a silence it did not choose.
            let ladder = builder.state.class_step_ladder_v1(&claim.class_id, ladder);
            let PalwClaimPhaseV2::DefaultDisputed { accused_daa, missing_event_index, accuser, accuser_exposure, resumed } =
                claim.phase.clone()
            else {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "MaterialDisclosedHeld" });
            };
            let recorded = builder.state.held_da_missing.get(&claim_id).copied();
            if missing_event_index != crate::palw_held_da_v1::PALW_HELD_DA_EVENT_INDEX_SENTINEL_V1
                || recorded != Some(disclosure.missing)
            {
                return Err(PalwStateV2Error::HeldDaSessionMismatch(claim_id));
            }
            crate::palw_held_da_v1::palw_held_da_check_disclosure_v1(
                &claim.execution_root,
                &disclosure.missing,
                &disclosure.binding,
                &disclosure.disclosure,
                ladder,
                builder.extras.prompt_ids_form_v1(),
            )
            .map_err(|e| PalwStateV2Error::HeldDaRefused { claim: claim_id, why: e.to_string() })?;
            // F1 (SPEC §4.6): an identity-faulty binding is not an answer about this claim's job.
            if let Some(why) = builder.da_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &disclosure.binding) {
                return Err(PalwStateV2Error::HeldDaRefused { claim: claim_id, why });
            }
            // **ADR-0111 Decision 4: a leaf's evidence is adjudicated, by the one-move verdict.**
            // Guilty: the executor convicted itself by answering — the claim voids `CourtFraud`
            // exactly as `ShardCourtAccused` voids it, and the accuser's stake comes back where every
            // door out of the disputed phase gives it back. Clear: the demand was wrong, and the
            // session closes refuted below. Evidence that does not adjudicate is no answer.
            if let crate::palw_held_da_v1::PalwHeldDisclosureV1::StepLeaf { evidence } = &disclosure.disclosure {
                let artifact_root =
                    builder.state.classes.get(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.artifact_root;
                let bound_to = crate::palw_shard_court_v1::PalwOneMoveClaimV2 {
                    execution_root: claim.execution_root,
                    class_id: claim.class_id,
                    artifact_root,
                };
                match evidence.verdict_at_v2(&bound_to, ladder, builder.extras.audit_2026_09_23_active).map_err(|e| {
                    PalwStateV2Error::HeldDaRefused { claim: claim_id, why: format!("the evidence does not adjudicate: {e}") }
                })? {
                    crate::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty => {
                        // A proven verdict (S-4) — refused past `palw_rcore_plus` above
                        // (`DaV1AnswerRetired`), so below it this is the void as it always was.
                        return builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, accuser, None);
                    }
                    crate::palw_shard_court_v1::PalwShardCourtVerdictV1::FalseAccusation => {}
                    crate::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection => {
                        return Err(PalwStateV2Error::HeldDaRefused {
                            claim: claim_id,
                            why: "a fused-attention leaf is tried by its dissection".to_string(),
                        });
                    }
                }
            }
            close_da_session_refuted_v2(builder, ctx, claim_id, &claim, accused_daa, accuser, accuser_exposure, *resumed)?;
        }
        // **ADR-0125 §7.3: a permit signed twice burns, and its bond pays the floor.**
        PalwConsensusObjectV2::RoundPermitEquivocated { evidence } => {
            builder.record_round_equivocation(ctx, evidence)?;
        }
        // ---- RFC-0006: layer-sharded panels (tags 91-93), dormant under `palw_tir_shard_v1` ----
        PalwConsensusObjectV2::TirShardPlanDeclared { class_id, s_l, s_p, signature: _ } => {
            palw_tir_shard_fold_v1::apply_plan_declared_v1(builder, ctx, class_id, *s_l, *s_p)?;
        }
        PalwConsensusObjectV2::TirShardReceiptLicensed { part } => {
            palw_tir_shard_fold_v1::apply_part_v1(builder, ctx, part)?;
        }
        PalwConsensusObjectV2::TirSeatReadinessProved { bond, class_id, shard, span, proof, signature: _ } => {
            // D2 (`palw_court_model_scope_v1`): no plaintext model leaves for a class registered past the fence.
            check_readiness_model_scope_v1(builder, ctx, class_id)?;
            palw_tir_shard_fold_v1::apply_readiness_v1(builder, ctx, bond, class_id, *shard, *span, proof)?;
        }
        // **ADR-0100 Decision 4: a class's shard plan.** The registrant's signature is the
        // acceptance layer's; here: the fence, a registered class with a registrant (a genesis
        // class has none and is not sharded), declared once, a count in range.
        PalwConsensusObjectV2::ClassShardPlanDeclared { class_id, shard_count, signature: _ } => {
            if builder.extras.shard_licensing.is_none() {
                return Err(PalwStateV2Error::ShardLicensingDormant);
            }
            let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
            if record.registrant_bond.is_none() {
                return Err(PalwStateV2Error::ShardPlanFromGenesisClass(*class_id));
            }
            if builder.state.class_shard_plans.contains_key(class_id) {
                return Err(PalwStateV2Error::ShardPlanAlreadyDeclared(*class_id));
            }
            if !crate::palw_shard_licensing_v1::palw_shard_count_in_range_v1(*shard_count) {
                return Err(PalwStateV2Error::ShardCountOutOfRange(*shard_count));
            }
            builder.write_class_shard_plan(
                *class_id,
                Some(crate::palw_shard_licensing_v1::PalwClassShardPlanV1 { shard_count: *shard_count, declared_daa: ctx.daa_score }),
            )?;
        }
        // **ADR-0100 Decision 4: a bond's shards of a class.** Only of a class it declared — the
        // exposure, the bound and the production proof all ride the class entry — only an Active
        // bond, only against the class's own plan. An empty list withdraws the row.
        PalwConsensusObjectV2::BondShardsDeclared { bond, class_id, shard_count, shards, signature: _ } => {
            if builder.extras.shard_licensing.is_none() {
                return Err(PalwStateV2Error::ShardLicensingDormant);
            }
            let record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
            if !matches!(record.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(*bond));
            }
            let plan = builder.state.class_shard_plans.get(class_id).copied().ok_or(PalwStateV2Error::ShardPlanMissing(*class_id))?;
            if plan.shard_count != *shard_count {
                return Err(PalwStateV2Error::ShardPlanMismatch { class: *class_id, declared: plan.shard_count, got: *shard_count });
            }
            if !record.capable_classes.contains(class_id) {
                return Err(PalwStateV2Error::BondShardsClassNotDeclared { bond: *bond, class: *class_id });
            }
            crate::palw_shard_licensing_v1::palw_bond_shards_shape_v1(*shard_count, shards)
                .map_err(PalwStateV2Error::BondShardsMalformed)?;
            {
                use crate::palw_native_resource_v1::NativeHeapV1;
                let bound = shards.native_heap_v1("bond shards incoming copy");
                builder
                    .native_reservation
                    .grow_v1(bound.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(bound))?)
                    .map_err(PalwStateV2Error::NativeResource)?;
            }
            builder.write_bond_shards((*bond, *class_id), (!shards.is_empty()).then(|| shards.clone()))?;
        }
        // **ADR-0100 Decision 4: one shard's part.** The receipts' signatures are the acceptance
        // layer's; here: the fence, a `PanelBound` claim that licenses by parts, the plan's count,
        // a shard not yet licensed, every receipt this claim's and a seat of THIS shard, then the
        // shard's quorum. Licensed: dissenting seats of this shard are charged as the whole-object
        // licence charges them, and the claim licenses in the block that lands its last shard.
        // Defaulted: the whole claim voids, as `ProducerDefaulted` voids it, where ADR-0065 D4 has
        // not retired that door.
        PalwConsensusObjectV2::ShardReceiptLicensed { part } => {
            let Some(params) = builder.extras.shard_licensing else {
                return Err(PalwStateV2Error::ShardLicensingDormant);
            };
            let claim_id = part.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            let PalwClaimPhaseV2::PanelBound { .. } = claim.phase else {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "ShardReceiptLicensed" });
            };
            // ADR-0147: the stratified draw seats no outsider, so an outsider-judged claim has no
            // part that could carry the outsider's answer. `validate_palw_v2` refuses the bundle
            // beside `palw_shard_licensing`; the fold says so too rather than licensing by a door
            // the rule never reached.
            if palw_claim_is_outsider_judged_v1(&builder.state, &claim, builder.extras.admission_independence_daa) {
                return Err(PalwStateV2Error::OutsiderJudgedClaimLicensedByParts(claim_id));
            }
            let plan =
                crate::palw_shard_licensing_v1::palw_claim_licenses_by_parts_v1(&builder.state, &claim_id, params.seats_per_shard)
                    .ok_or(PalwStateV2Error::NotLicensedByParts(claim_id))?;
            if part.shard_count != plan.shard_count {
                return Err(PalwStateV2Error::ShardPlanMismatch {
                    class: claim.class_id,
                    declared: plan.shard_count,
                    got: part.shard_count,
                });
            }
            if part.shard_index >= plan.shard_count {
                return Err(PalwStateV2Error::ShardIndexOutOfRange { shard: part.shard_index, count: plan.shard_count });
            }
            let mut progress = match builder.state.shard_licensing.get(&claim_id) {
                Some(progress) => progress.clone(),
                None => crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1::new(plan.shard_count)
                    .map_err(|_| PalwStateV2Error::ShardCountOutOfRange(plan.shard_count))?,
            };
            if progress.is_licensed(part.shard_index) {
                return Err(PalwStateV2Error::ShardAlreadyLicensed { claim: claim_id, shard: part.shard_index });
            }
            let seats = builder.state.panels.get(&claim_id).map(|panel| panel.seats.clone()).ok_or(PalwStateV2Error::EmptyPanel)?;
            let slice = crate::palw_shard_licensing_v1::palw_panel_shard_slice_v1(
                &seats,
                plan.shard_count,
                params.seats_per_shard,
                part.shard_index,
            )
            .ok_or(PalwStateV2Error::NotLicensedByParts(claim_id))?;
            let shard = part.shard_index;
            let refused = |why: String| PalwStateV2Error::ShardPartRefused { claim: claim_id, shard, why };
            if let Some(stray) = part.receipts.iter().find(|receipt| receipt.claim != claim_id) {
                return Err(refused(format!("a receipt names claim {}", stray.claim)));
            }
            let verdicts: Vec<(PalwBondKeyV2, crate::palw_panel_v2::PalwReceiptVerdictV2)> =
                part.receipts.iter().map(|receipt| (receipt.seat_bond, receipt.verdict)).collect();
            let quorum = crate::palw_shard_licensing_v1::palw_shard_quorum_v1(slice, &verdicts, params.quorum_per_shard)
                .map_err(|e| refused(e.to_string()))?;
            let seat_verdicts = palw_seat_verdicts_of_v2(&part.receipts);
            match quorum {
                crate::palw_shard_licensing_v1::PalwShardQuorumV1::Licensed { .. } => {
                    // **2026-09-23 audit #14: a part's `Valid` signers lock, as every whole-object
                    // door's do.** Below the fence this arm wrote no lock at all, so a claim that
                    // licensed by parts reached `Final` with nothing slashable behind any signature
                    // and a `PanelFalseValid` had no lock to take — `G` wholly unbacked, on the one
                    // door that never called the ledger. Past it the part is the door: a part whose
                    // `Valid` signers cannot post the shard price is INERT (nothing moves; the
                    // shard stays open for a backed part, else the receipt window voids the claim),
                    // and a backed part locks each signer as it lands. The lock's clocks run from
                    // here and are restarted at `Final` by `persist_panel_liability`, which reads
                    // every lock on the claim, so the parts that landed first are carried too.
                    let door = PalwLicenceDoorV1::ShardPart { quorum_per_shard: params.quorum_per_shard };
                    if builder.extras.audit_2026_09_23_active
                        && !builder.receipt_set_is_backed(claim_id, &claim, &part.receipts, ctx.daa_score, door)
                    {
                        return Ok(());
                    }
                    builder.slash_dissenting_seats(&claim_id, &claim, &seat_verdicts, true)?;
                    // ADR-0124 Decision 2: a shard's `Valid` seats are credited as the part lands.
                    builder.credit_seat_receipts(claim_id, &part.receipts, ctx.daa_score)?;
                    if builder.extras.audit_2026_09_23_active {
                        builder.lock_valid_receipts(claim_id, &claim, &part.receipts, ctx.daa_score, door)?;
                    }
                    progress.mark(shard).map_err(|e| refused(e.to_string()))?;
                    if progress.is_complete() {
                        // The claim licenses; `write_claim` drops the progress row with the phase.
                        builder.license_claim(claim_id, claim, ctx.daa_score)?;
                    } else {
                        builder.write_shard_licensing(claim_id, Some(progress))?;
                    }
                }
                crate::palw_shard_licensing_v1::PalwShardQuorumV1::ProducerUnavailable { .. } => {
                    if builder.unavailable_abstains {
                        return Err(PalwStateV2Error::ProducerDefaultRetired(claim_id));
                    }
                    builder.slash_dissenting_seats(&claim_id, &claim, &seat_verdicts, false)?;
                    builder.void_and_slash(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
                }
                crate::palw_shard_licensing_v1::PalwShardQuorumV1::Short { valid, unavailable, needed } => {
                    return Err(PalwStateV2Error::ShardPartShort { claim: claim_id, shard, valid, unavailable, needed });
                }
            }
        }
        PalwConsensusObjectV2::ModelSeed { line_id, seeder, msk_seed, sink_index: _ } => {
            model_seed_v1(builder, ctx, line_id, seeder, *msk_seed)?;
        }
        // ADR-0152-adjacent (Activation Pool): a sink-bound top-up.
        PalwConsensusObjectV2::ActivationPoolFunded { class_id, amount, sink_index: _ } => {
            apply_activation_pool_funded_v1(builder, ctx, class_id, *amount)?;
        }
        // ---- ADR-0088: the model registry ---------------------------------------------------
        PalwConsensusObjectV2::ModelLineFounded { class_id, name, founder, root, signature: _ } => {
            apply_model_line_founded(builder, ctx, class_id, name, founder, root)?;
        }
        PalwConsensusObjectV2::ModelVersionPublished {
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
            signature: _,
        } => {
            let declared = crate::palw_model_lines_v1::PalwModelVersionV1 {
                root: *root,
                parent: *parent,
                adopted_from: *adopted_from,
                runtime_hash: *runtime_hash,
                dataset_commitment: *dataset_commitment,
                training_config_hash: *training_config_hash,
                notes_hash: *notes_hash,
                published_daa: ctx.daa_score,
                published_by: None,
                status: crate::palw_model_lines_v1::PalwVersionStatusV1::Current,
                usage: Default::default(),
            };
            apply_model_version_published(builder, ctx, line_id, *version, declared, *preview)?;
        }
        PalwConsensusObjectV2::ModelVersionPromoted { line_id, version, signature: _ } => {
            apply_model_version_promoted(builder, ctx, line_id, *version)?;
        }
        PalwConsensusObjectV2::ModelVersionWithdrawn { line_id, version, signature: _ } => {
            apply_model_version_withdrawn(builder, line_id, *version)?;
        }
        PalwConsensusObjectV2::ModelLineBenefitsDeclared { line_id, tiers, cadence_daa, expires_daa, signature: _ } => {
            apply_model_line_benefits(builder, ctx, line_id, tiers, *cadence_daa, *expires_daa)?;
        }
        PalwConsensusObjectV2::ModelLineRolesSet { line_id, developer, maintainer, contributor_permille_of_leg, signature: _ } => {
            apply_model_line_roles(builder, line_id, *developer, *maintainer, *contributor_permille_of_leg)?;
        }
        PalwConsensusObjectV2::ModelLineOwnerTransferred { line_id, new_owner, signature: _ } => {
            apply_model_line_transfer(builder, line_id, new_owner)?;
        }
        PalwConsensusObjectV2::ModelLineRetired { line_id, signature: _ } => {
            apply_model_line_retired(builder, ctx, line_id)?;
        }
        PalwConsensusObjectV2::ModelProposalPosted { line_id, root, note_hash, by, signature: _ } => {
            apply_model_proposal_posted(builder, ctx, line_id, root, note_hash, by)?;
        }
        PalwConsensusObjectV2::ModelProposalClosed { line_id, proposal_id, signature: _ } => {
            apply_model_proposal_closed(builder, line_id, proposal_id)?;
        }
        PalwConsensusObjectV2::ModelEvaluationPosted {
            line_id,
            version,
            evaluator_id,
            score_permille,
            report_hash,
            by,
            signature: _,
        } => {
            apply_model_evaluation_posted(builder, ctx, line_id, *version, evaluator_id, *score_permille, report_hash, by)?;
        }
        PalwConsensusObjectV2::BondCapabilityDeclared { bond, capable_classes, .. } => {
            let record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?.clone();
            // **Only classes this chain has actually registered.** A declaration naming an
            // unregistered id is either a typo or a bet on a future registration, and the draw
            // would carry it silently until that id appeared — at which point a seat would be
            // volunteered for a class its operator declared before the class existed.
            //
            // (ADR-0071 SA-4's second half, and it was already here — recorded rather than
            // re-implemented, because a rule asserted twice is a rule two readers can change
            // apart.)
            if let Some(unknown) = capable_classes.iter().find(|id| !builder.state.classes.contains_key(id)) {
                return Err(PalwStateV2Error::MissingClass(*unknown));
            }
            if builder.capability_bound {
                // The bond's own capability exposure is excluded from `already` deliberately: the
                // new set REPLACES the old one (see below), so what the previous declaration
                // reserved is released by this very write and charging both would refuse an
                // operator for exposure it is in the act of giving back. Everything else the bond
                // backs — live claims and its own class registrations — is counted, on the
                // ADR-0056 Decision 3 rule that the ceiling applies to the SUM.
                // ADR-0152 (the S review's L4): past `palw_rcore_plus` what the bond backs is the one
                // ledger plus its accuser ledger — the invariant `committed + accuser ≤ C` this
                // declaration's 100% check extends to (capability exposure is non-slashable
                // standing, priced against the whole collateral as before).
                let already = if builder.params.rcore_plus_active_at(ctx.daa_score) {
                    builder.committed_at(bond, ctx.daa_score).saturating_add(builder.read().accuser_ledger_v1(bond, ctx.daa_score))
                } else {
                    builder
                        .state
                        .reserved_exposure(bond)
                        .checked_add(builder.state.registration_exposure(bond))
                        .ok_or(PalwStateV2Error::Overflow("total exposure"))?
                };
                check_capable_classes_declared_v1(
                    &builder.state,
                    *bond,
                    capable_classes,
                    matches!(record.status, PalwBondStatusV2::Retiring { .. }),
                    already,
                    record.collateral,
                )?;
            }
            // **SA-1's second half: a declaration REPLACES, it does not grow.** This assignment is
            // the whole of it and it predates the amendment — recorded here rather than changed,
            // because the amendment asks for the property and an `extend` slipped in later would
            // silently unbound the set the check above just bounded. It is also what releases
            // SA-2's reservation for every class the new set drops: the exposure is DERIVED from
            // this field (`palw_bond_capability_exposure_v1`), so a shrinking set is a release with
            // no second site to forget.
            let mut declared = record;
            declared.capable_classes = capable_classes.clone();
            builder.write_bond(*bond, Some(declared))?;
        }
        PalwConsensusObjectV2::BondRetireRequested { bond, .. } => {
            let record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?.clone();
            match record.status {
                PalwBondStatusV2::Retiring { .. } => return Err(PalwStateV2Error::BondAlreadyRetiring(*bond)),
                PalwBondStatusV2::Active => {
                    if builder.extras.objective_offence_at(ctx.daa_score) {
                        let locked = builder.slashable_live_locked(bond, ctx.daa_score);
                        if locked > 0 {
                            return Err(PalwStateV2Error::BondRetireWhileSlashableLocked { bond: *bond, locked });
                        }
                    }
                    let mut retiring = record;
                    retiring.status = PalwBondStatusV2::Retiring {
                        since_daa: ctx.daa_score,
                        settled_at_since: builder.state.settled_attempt_finals,
                    };
                    // **ADR-0071 SA-2: retirement releases the declaration's reservation.** A
                    // retiring bond is already unseatable (`palw_bond_may_take_work_v2` refuses a
                    // non-Active status), so dropping the set costs it nothing it still had — and
                    // leaving the set behind would hold its collateral against a promise the draw
                    // will never call on. The exposure is derived from this field, so clearing it
                    // IS the release.
                    if builder.capability_bound {
                        retiring.capable_classes.clear();
                    }
                    builder.write_bond(*bond, Some(retiring))?;
                }
            }
        }
        // **RFC-0003 (tag 68): a generative class registration**, past `palw_gen_v1` (the lock above
        // refuses it below). The pipeline admission ran at acceptance (ADR-0049 Decision H: no graph
        // walk inside the transition); the fold derives what it keeps from the carried class — the
        // `gen_classes` row by the one function admission derives it with — and folds the rest through
        // the legacy registration's own body. A class with a dissected commit point (a cone that
        // reduces over the history, F7 composed) owes its court's terminal move, as an IR class does
        // (`fused_attention`); nothing is held; and a pipeline opens no registry lifecycle row in v1
        // (the registry's work model is a text model's).
        // **RFC-0003 §I.4: a tensor claim's commitment** — the fences, the bond, the class, the job against the
        // class, the work (the chain's own count), the class's capacity, the work identity, the reservation; then
        // a weightless claim that is any claim of its class (`palw_gen_claim_fold_v1`).
        PalwConsensusObjectV2::GenTensorCommitted {
            claim,
            class_id,
            bond,
            executor_pubkey,
            work_leaves,
            prompt_token_ids,
            trace_root,
            output_root,
            execution_root,
            job_pin,
            job,
        } => {
            palw_gen_claim_fold_v1::apply_gen_tensor_commitment_v1(
                builder,
                ctx,
                palw_gen_claim_fold_v1::PalwGenCommitV1 {
                    claim_id: claim,
                    class_id,
                    bond,
                    executor_pubkey,
                    work_leaves: *work_leaves,
                    prompt_token_ids,
                    trace_root,
                    output_root,
                    execution_root,
                    job_pin,
                    job,
                },
            )?;
        }
        PalwConsensusObjectV2::ClassRegisteredGenV1 {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            share_permille,
            activation_daa,
            admission,
        } => {
            // **Lane PA, G-3 (`palw_audit_1004_v1`)**: a generative registration is sized like an IR one — one such registration a block
            // (the counter is shared: the acceptance walk drops a second by name first, this is the second lock).
            if builder.params.audit_1004_active_at(ctx.daa_score) {
                if builder.tir_registrations >= PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1 {
                    return Err(PalwStateV2Error::TirRegistrationsPerBlockExceeded {
                        class: *class_id,
                        max: PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1,
                    });
                }
                builder.tir_registrations += 1;
            }
            let record = crate::palw_gen_class_v1::palw_gen_class_record_v1(&admission.class, artifact_root)
                .map_err(|_| PalwStateV2Error::GenObjectRefused("the carried pipeline does not decode"))?;
            if record.class_id != *class_id {
                return Err(PalwStateV2Error::GenObjectRefused("the declared class id is not the carried class's"));
            }
            apply_class_registration_v1(
                builder,
                ctx,
                object,
                PalwClassRegistrationFoldV1 {
                    class_id,
                    artifact_root,
                    slash_value_per_pwu,
                    pwu_rule,
                    initial_target,
                    share_permille,
                    activation_daa,
                    registrant: Some(admission.registrant_bond),
                    fused_attention: !record.dissected.is_empty(),
                    held: false,
                    work: None,
                },
            )?;
            builder.write_gen_class(*class_id, Some(record))?;
        }
        // **RFC-0002 Phase F (tag 61): an IR class registration**, past `palw_tir_v1` (the lock above
        // refuses it below). Admission v10 ran at acceptance (ADR-0049 Decision H: no graph walk
        // inside the transition); the fold derives what it keeps from the carried class — the
        // `tir_classes` row by the one function v10 derives it with, and the registry work from the
        // program — and folds the rest through the legacy registration's own body. A class with a
        // dissected commit point (spec 04b §9.5.1: a cone that reduces over the history) owes its
        // court's terminal move — a root claim at a dissected leaf, an acquitting close at any other
        // — exactly as a legacy fused class does (`fused_attention`, ADR-0082 C-5; RFC-0002 F7). It
        // records no held ladder (v10 admits no held program).
        PalwConsensusObjectV2::ClassRegisteredTirV1 {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            share_permille,
            activation_daa,
            admission,
        } => {
            // One a block: the acceptance walk drops a second by name first; the second lock.
            if builder.tir_registrations >= PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1 {
                return Err(PalwStateV2Error::TirRegistrationsPerBlockExceeded {
                    class: *class_id,
                    max: PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1,
                });
            }
            builder.tir_registrations += 1;
            let (record, program) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&admission.class, artifact_root)
                .map_err(|_| PalwStateV2Error::TirRegistrationRefused("the carried IR program does not decode"))?;
            if record.facts.class_id != *class_id {
                return Err(PalwStateV2Error::TirRegistrationRefused("the declared class id is not the carried class's"));
            }
            // RFC-0002 Phase F's second IR fence: past it the registry records the `Select`-arm credit
            // (each arm-only region at the smaller arm's work); below it, the release's vector.
            let min_select_arms = builder.params.tir_fence2_active_at(ctx.daa_score);
            let work = || crate::palw_tir_work_v1::palw_tir_model_work_v2(&program, &admission.canonical, min_select_arms).ok();
            apply_class_registration_v1(
                builder,
                ctx,
                object,
                PalwClassRegistrationFoldV1 {
                    class_id,
                    artifact_root,
                    slash_value_per_pwu,
                    pwu_rule,
                    initial_target,
                    share_permille,
                    activation_daa,
                    registrant: Some(admission.registrant_bond),
                    fused_attention: !record.dissected.is_empty(),
                    held: false,
                    work: Some(&work),
                },
            )?;
            if builder.extras.model_court_window_active && !record.dissected.is_empty() {
                let window = builder
                    .extras
                    .class_court_windows
                    .get(class_id)
                    .copied()
                    .filter(|window| *window >= builder.params.window_court())
                    .ok_or(PalwStateV2Error::ClassCourtWindowMissing(*class_id))?;
                builder.write_class_court_window(*class_id, Some(window))?;
            }
            builder.write_tir_class(*class_id, Some(record))?;
            // **RFC-0007 Part II**: past `palw_witness_manifest_v1` the class pins its witness profile (infallible).
            palw_mesh_fold_v1::record_witness_profile_v1(builder, ctx, class_id, &program, admission.class.layout.max_context)?;
        }
        PalwConsensusObjectV2::ClassSemanticsDeclaredV1 { declaration, .. } => {
            if !builder.extras.class_semantics_active {
                return Err(PalwStateV2Error::ImmutableModelUpdate("semantic declaration before its independent fence"));
            }
            declaration.check().map_err(PalwStateV2Error::ImmutableModelUpdate)?;
            let class = builder
                .state
                .class(&declaration.class_id)
                .ok_or(PalwStateV2Error::ImmutableModelUpdate("semantic declaration class is unregistered"))?;
            if class.artifact_root != declaration.artifact_root || class.registrant_bond.is_none() {
                return Err(PalwStateV2Error::ImmutableModelUpdate(
                    "semantic declaration must name the originally registered artifact and registrant",
                ));
            }
            if let Some(relation) = declaration.prefix_state_relation {
                let profile = builder
                    .state
                    .fp_work_profile_of(&declaration.class_id)
                    .filter(|p| p.shape_profile_id() == declaration.class_id)
                    .ok_or(PalwStateV2Error::ImmutableModelUpdate(
                        "a family state relation needs the publicly certified canonical class graph",
                    ))?;
                relation.check_profile(profile).map_err(PalwStateV2Error::ImmutableModelUpdate)?;
            }
            let mut next = declaration.as_bound_row(ctx.daa_score);
            if let Some(previous) = builder.state.class_semantics.get(&declaration.class_id) {
                next.bound_from_daa = previous.bound_from_daa;
                let mut expected = previous.clone();
                if expected.token_table_root.is_none() {
                    expected.token_table_root = next.token_table_root;
                }
                if expected.rendering_rule.is_none() {
                    expected.rendering_rule = next.rendering_rule;
                }
                if expected.raw_tokenizer_bytes.is_none() {
                    expected.raw_tokenizer_bytes = next.raw_tokenizer_bytes;
                }
                if expected.tokenizer_key_events.is_none() {
                    expected.tokenizer_key_events = next.tokenizer_key_events;
                }
                if expected.prefix_state_relation.is_none() {
                    expected.prefix_state_relation = next.prefix_state_relation;
                }
                if expected != next {
                    return Err(PalwStateV2Error::ImmutableModelUpdate(
                        "registered semantic identities and a filled table root are immutable",
                    ));
                }
            }
            builder.write_class_semantics_v1(declaration.class_id, Some(next))?;
        }
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu,
            pwu_rule,
            initial_target,
            share_permille,
            activation_daa,
            // The admission carriage is the ACCEPTANCE layer's (ADR-0049 Decision H): running
            // `verify_class_admission_v2` here would put a graph walk inside a pure transition,
            // which is the same boundary `BondRegistered` draws for signatures. The transition
            // enforces referential integrity and the lattice; the gate decides whether the object
            // may be folded at all.
            //
            // Its `registrant_bond` IS read here, and that is not the graph walk: whose bond pays
            // the ADR-0056 Decision 3 reservation is referential integrity, the transition's own
            // business, and the only place the fact exists.
            admission,
        } => {
            // One body for both registration doors (`apply_class_registration_v1`): the carriage's
            // registrant, and the three facts the fold reads off the profile it carries.
            let work = admission.as_ref().map(|carriage| {
                move || crate::palw_model_registry_v1::palw_model_work_from_carriage_v1(&carriage.profile, &carriage.canonical)
            });
            apply_class_registration_v1(
                builder,
                ctx,
                object,
                PalwClassRegistrationFoldV1 {
                    class_id,
                    artifact_root,
                    slash_value_per_pwu,
                    pwu_rule,
                    initial_target,
                    share_permille,
                    activation_daa,
                    registrant: admission.as_ref().map(|carriage| carriage.registrant_bond),
                    fused_attention: admission.as_ref().is_some_and(|carriage| {
                        crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&carriage.profile)
                    }),
                    held: admission
                        .as_ref()
                        .is_some_and(|carriage| crate::palw_state_chunk_map::palw_profile_is_held_v4(&carriage.profile)),
                    work: work.as_ref().map(|f| f as &dyn Fn() -> Option<crate::palw_model_registry_v1::PalwModelWorkV1>),
                },
            )?;
            if builder.extras.model_court_window_active
                && admission
                    .as_ref()
                    .is_some_and(|carriage| crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&carriage.profile))
            {
                let window = builder
                    .extras
                    .class_court_windows
                    .get(class_id)
                    .copied()
                    .filter(|window| *window >= builder.params.window_court())
                    .ok_or(PalwStateV2Error::ClassCourtWindowMissing(*class_id))?;
                builder.write_class_court_window(*class_id, Some(window))?;
            }
        }
        // **ADR-0078 Decision 4: a derivation is committed beside its claim; the thing never
        // rides.** The chain checks what it can check — the claim exists on this chain (any phase
        // from committed onward: a derivation of a claim that later voids is a derivation of a
        // voided claim, and says so when read), the declared output root is the claim's own, the
        // signer is the claim's executor, the `(claim, transformer)` pair is new, and the claim is
        // under its cap — and records the names. It credits no weight, no payment and no exposure
        // (X5): the object is a statement priced by its fee, and the chain cannot run the
        // transformer (Decision 5), so it does not pretend to.
        PalwConsensusObjectV2::DerivedArtifactV1 { object, signature } => {
            use crate::palw_derived_v1::{PALW_DERIVED_MAX_PER_CLAIM, PalwDerivedKeyV1, PalwDerivedRowV1, check_derived_carriage_v1};
            // The signature is checked for LENGTH here as well as in the ride list — the second
            // lock the arms above describe, and the one that keeps Decision 1's "under any size"
            // true if anyone ever reaches this transition from a path that is not a 0x4b carrier.
            check_derived_carriage_v1(object, signature).map_err(PalwStateV2Error::DerivedShapeRefused)?;
            let claim = builder.state.claims.get(&object.claim_id).ok_or(PalwStateV2Error::DerivedClaimMissing(object.claim_id))?;
            if !matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }) {
                return Err(PalwStateV2Error::DerivedClaimIsNotFreePrompt(object.claim_id));
            }
            if claim.output_root != object.output_root {
                return Err(PalwStateV2Error::DerivedOutputRootMismatch {
                    claim: object.claim_id,
                    declared: object.output_root,
                    committed: claim.output_root,
                });
            }
            // The same comparison `FreePromptCommitted` makes: the acceptance layer proved the
            // object is signed by the key it declares; only the registry can say whether that key
            // is the claim's bond's.
            let bond_record = builder.state.bonds.get(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?;
            if bond_record.pubkey != object.executor_pubkey {
                return Err(PalwStateV2Error::DerivedSignerIsNotTheExecutor { claim: object.claim_id });
            }
            let key = PalwDerivedKeyV1 { claim: object.claim_id, transformer: object.transformer_id };
            if builder.state.derived_artifacts.contains_key(&key) {
                return Err(PalwStateV2Error::DuplicateDerivation { claim: object.claim_id, transformer: object.transformer_id });
            }
            let held = builder.state.derived_artifacts.range(PalwDerivedKeyV1::claim_range(object.claim_id)).count();
            if held >= PALW_DERIVED_MAX_PER_CLAIM {
                return Err(PalwStateV2Error::TooManyDerivations { claim: object.claim_id, max: PALW_DERIVED_MAX_PER_CLAIM });
            }
            builder.write_derived_artifact(key, Some(PalwDerivedRowV1::from_object(object, ctx.daa_score)))?;
        }
        PalwConsensusObjectV2::FamilyCertified { evidence } => {
            let lane = evidence.lane();
            let got = evidence.vector_count();
            if got > PALW_CERTIFICATION_MAX_VECTORS {
                return Err(PalwStateV2Error::TooManyDrillVectors { got, max: PALW_CERTIFICATION_MAX_VECTORS });
            }
            // **The court grades; nothing else vouches** (ADR-0075 Decision 2). The family that
            // enters the state is the one the shipped grader produced from these vectors, so a
            // node that could not convict the planted faults never records a family it could
            // not prosecute — the same seal ADR-0069 put on the compile-time set.
            let family = evidence.grade().map_err(|e| PalwStateV2Error::CertificationRefused { lane, why: e.to_string() })?;
            let digest = family.digest();
            if builder.state.certified_family(lane, &digest).is_some() {
                return Err(PalwStateV2Error::FamilyAlreadyCertified { lane, digest });
            }
            builder.write_certified_family(lane, digest, Some(PalwCertifiedFamilyStateV2 { family, certified_daa: ctx.daa_score }))?;
        }
        PalwConsensusObjectV2::ClassLaneCertified { class_id, lane, profile } => {
            let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?.clone();
            if !matches!(record.status, PalwClassStatusV2::Active) {
                return Err(PalwStateV2Error::CertificationNeedsActiveClass { class: *class_id });
            }
            // A class id IS its profile's id (`verify_class_admission_v2`), so the profile the
            // object carries is checkable against the class it names without the state holding
            // profiles — and the kernels the class reaches are read off that profile, exactly as
            // the registration gate reads them.
            profile.validate_shape().map_err(|e| PalwStateV2Error::CertificationProfileInvalid(e.to_string()))?;
            let derived = profile.shape_profile_id();
            if derived != *class_id {
                return Err(PalwStateV2Error::CertificationProfileIsNotTheClass { class: *class_id, derived });
            }
            let reachable = crate::palw_class_admission_v2::reachable_kernels_v1(profile);
            let covering = builder
                .state
                .certified_family_records(*lane)
                .iter()
                .find(|(_, record)| reachable.is_subset(&record.family.kernel_ids))
                .map(|(digest, _)| *digest)
                .ok_or(PalwStateV2Error::NoCertifiedFamilyCovers { class: *class_id, lane: *lane })?;
            match lane {
                PalwCertifiedLaneV1::FreePrompt => {
                    // **ADR-0145 §5: past the derived-work fence this object is also how a class
                    // publishes the graph its work is derived from.**
                    //
                    // The graph is already in hand and already checked: `profile.validate_shape()`
                    // ran above and `shape_profile_id() == class_id` — a class id IS its profile's
                    // id — so the row cannot be wrong whoever carried it, which is why publishing
                    // needs no authority and no signature.
                    //
                    // Recording it here rather than at registration is what makes arming the fence
                    // possible at all. Every class the free-prompt lane can pay already has to
                    // reach this object (`FreePromptLaneUncertified` above), and a class that
                    // reached it BEFORE the fence left no row — so past the fence a certified class
                    // may carry this object a second time for the sole purpose of publishing its
                    // graph. That is the one relaxation of the "certify once" rule, it is fenced,
                    // and it changes nothing else: the certification row itself is left exactly as
                    // the first object wrote it, including its `certified_daa`, because a class's
                    // certification date is a fact about when it was drilled and re-carrying a
                    // profile is not a re-drill.
                    let already = builder.state.fp_certified_classes.contains_key(class_id);
                    let publishing_profile =
                        builder.extras.fp_derived_work_at(ctx.daa_score) && !builder.state.fp_work_profiles.contains_key(class_id);
                    if already && !publishing_profile {
                        return Err(PalwStateV2Error::ClassLaneAlreadyCertified { class: *class_id, lane: *lane });
                    }
                    if !already {
                        builder.write_fp_certified_class(
                            *class_id,
                            Some(PalwClassLaneCertificationV2 { family_digest: covering, certified_daa: ctx.daa_score }),
                        )?;
                    }
                    if builder.extras.fp_derived_work_at(ctx.daa_score) {
                        builder.write_fp_work_profile(*class_id, Some(profile.clone()))?;
                    }
                }
                PalwCertifiedLaneV1::Attempt => seat_attempt_lane_class_v1(builder, ctx, class_id)?,
            }
        }
        // **RFC-0002 Phase F (tag 63): an IR class bound to a certified family.** The legacy attempt
        // lane's rule over the IR class: an Active class, the carried class hashing to its id, a
        // registered IR class, and one family certified for the attempt lane covering every primitive
        // the program reaches — then the lane's own seat.
        PalwConsensusObjectV2::ClassLaneCertifiedTirV1 { class_id, artifact_root, class } => {
            let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?.clone();
            if !matches!(record.status, PalwClassStatusV2::Active) {
                return Err(PalwStateV2Error::CertificationNeedsActiveClass { class: *class_id });
            }
            let derived = class.class_id(artifact_root);
            if derived != *class_id || record.artifact_root != *artifact_root || !builder.state.tir_classes.contains_key(class_id) {
                return Err(PalwStateV2Error::CertificationProfileIsNotTheClass { class: *class_id, derived });
            }
            let program = class.decode_program().map_err(|e| PalwStateV2Error::CertificationProfileInvalid(e.to_string()))?;
            let reachable = crate::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&program);
            if !builder
                .state
                .certified_family_records(PalwCertifiedLaneV1::Attempt)
                .iter()
                .any(|(_, record)| reachable.is_subset(&record.family.kernel_ids))
            {
                return Err(PalwStateV2Error::NoCertifiedFamilyCovers { class: *class_id, lane: PalwCertifiedLaneV1::Attempt });
            }
            seat_attempt_lane_class_v1(builder, ctx, class_id)?;
        }
        PalwConsensusObjectV2::ObjectChunk { group, index, count, bytes } => {
            if *count == 0 || *count > PALW_OBJECT_CHUNK_MAX_COUNT {
                return Err(PalwStateV2Error::ChunkCountOutOfRange { count: *count, max: PALW_OBJECT_CHUNK_MAX_COUNT });
            }
            if *index >= *count {
                return Err(PalwStateV2Error::ChunkIndexOutOfRange { index: *index, count: *count });
            }
            if bytes.is_empty() || bytes.len() > PALW_OBJECT_CHUNK_MAX_BYTES {
                return Err(PalwStateV2Error::ChunkTooLarge { bytes: bytes.len(), max: PALW_OBJECT_CHUNK_MAX_BYTES });
            }
            // Preserve semantic refusal before the local capacity check, then
            // reserve the actual group copy/assembly before it allocates.
            if let Some(pending) = builder.state.pending_chunks.get(group) {
                if pending.count != *count {
                    return Err(PalwStateV2Error::ChunkGroupIncoherent { group: *group });
                }
                if pending.parts.contains_key(index) {
                    return Err(PalwStateV2Error::ChunkDuplicate { group: *group, index: *index });
                }
            }
            builder.reserve_chunk_assembly_v1(group, bytes)?;
            let mut pending = match builder.state.pending_chunks.get(group) {
                Some(pending) => {
                    if pending.count != *count {
                        return Err(PalwStateV2Error::ChunkGroupIncoherent { group: *group });
                    }
                    if pending.parts.contains_key(index) {
                        return Err(PalwStateV2Error::ChunkDuplicate { group: *group, index: *index });
                    }
                    pending.clone()
                }
                None => {
                    // Room is made by evicting groups past their TTL — in key order, so every
                    // node evicts the same ones — and refused when none can be made.
                    let expired: Vec<Hash64> = builder
                        .state
                        .pending_chunks
                        .iter()
                        .filter(|(_, p)| p.opened_daa.saturating_add(PALW_OBJECT_CHUNK_TTL_DAA) < ctx.daa_score)
                        .map(|(k, _)| *k)
                        .collect();
                    for key in expired {
                        builder.write_pending_chunks(key, None)?;
                    }
                    if builder.state.pending_chunks.len() >= PALW_OBJECT_CHUNK_MAX_GROUPS {
                        return Err(PalwStateV2Error::TooManyPendingChunkGroups { max: PALW_OBJECT_CHUNK_MAX_GROUPS });
                    }
                    PalwPendingChunksV2 { count: *count, opened_daa: ctx.daa_score, parts: BTreeMap::new() }
                }
            };
            pending.parts.insert(*index, bytes.clone());
            if pending.parts.len() < pending.count as usize {
                builder.write_pending_chunks(*group, Some(pending))?;
            } else {
                // The group is complete: the object it carried is applied HERE, in this block, by
                // the same arms every directly-carried object goes through.
                let mut whole = Vec::with_capacity(pending.parts.values().map(Vec::len).sum());
                for i in 0..pending.count {
                    whole.extend_from_slice(&pending.parts[&i]);
                }
                let computed = palw_object_chunk_group_id_v1(&whole);
                if computed != *group {
                    return Err(PalwStateV2Error::ChunkGroupHashMismatch { group: *group, computed });
                }
                builder.reserve_chunk_decode_v1(&whole)?;
                let inner: PalwConsensusObjectV2 =
                    borsh::from_slice(&whole).map_err(|e| PalwStateV2Error::ChunkedObjectUndecodable(e.to_string()))?;
                // Below `palw_task_heads_v1` a `Head` variant does not decode on the int-12 build: here it is the same error, first.
                if crate::palw_task_heads_v1::palw_object_needs_task_heads_v1(&inner)
                    && !builder.params.task_heads_active_at(ctx.daa_score)
                {
                    return Err(PalwStateV2Error::ChunkedObjectUndecodable("a Head variant below palw_task_heads_v1".into()));
                }
                // **A-2 uniformity: an inner kind whose owning fence is not in force at this block is bytes the live build cannot
                // decode** — refused exactly as it refuses them, in its words, before any kind rule reads it.
                if !palw_fold_kind_in_force_v1(builder, ctx, &inner) {
                    return Err(PalwStateV2Error::ChunkedObjectUndecodable(
                        crate::palw_lifecycle_objects_v2::palw_lifecycle_unknown_tag_reason_v1(&whole),
                    ));
                }
                if !palw_chunked_object_kind_admitted_v1(&inner) {
                    return Err(PalwStateV2Error::ChunkedObjectKindNotAllowed);
                }
                builder.write_pending_chunks(*group, None)?;
                apply_object(builder, ctx, &inner)?;
            }
        }
        PalwConsensusObjectV2::ClassFrozen(frozen_object) => {
            let class_id = &frozen_object.class_id();
            let certificate = frozen_object.certificate();
            let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?.clone();
            if let PalwClassStatusV2::Frozen { .. } = record.status {
                return Err(PalwStateV2Error::FrozenClass(*class_id));
            }
            // **ADR-0039 W6′: the liveness floor may never be absent.** Nothing refused freezing
            // it, and on a `ConsensusV2` network the consequence is total: the attempt lane is the
            // only block type, admission refuses a frozen class, and BASE-0 is the class every
            // operator can run — so one accepted `ClassFrozen` naming the floor ends the chain,
            // with no path back because the object that would unfreeze it needs a block. A
            // contradiction certificate against the floor is real evidence and it belongs on
            // chain, but its remedy is a new ruleset, not a switch any block may throw.
            if *class_id == builder.params.base_class_id {
                return Err(PalwStateV2Error::BaseClassMayNotFreeze(*class_id));
            }
            // The structural half of the proof, checked HERE because it is the half a pure
            // transition can decide. Signatures are the acceptance layer's, the same split
            // `BondRegistered` uses — and `adjudicate_class_contradiction_v1` is the function
            // that runs the whole thing where a verifier is in hand.
            check_class_contradiction_shape_v2(*class_id, certificate)?;
            // Decision 3: a frozen class is no longer a live registration, so its registrant's
            // reservation returns. The class keeps its share (freeze moves no permille) — the
            // exposure is the price of the REGISTRY slot, and a frozen class occupies none.
            if let Some(bond) = record.registrant_bond {
                builder.move_registration_exposure(bond, false)?;
            }
            let mut frozen = record;
            frozen.status = PalwClassStatusV2::Frozen { since_daa: ctx.daa_score };
            builder.write_class(*class_id, Some(frozen))?;
            // **ADR-0152-adjacent (Activation Pool): a proven-bad class stops attracting preparers.**
            // Its budgets move to the row's own `withheld` — never into `panel_reserve_sompi`, whose
            // number is ADR-0124's withheld escrow (the review's C10) — and nothing is refunded: a
            // top-up is a donation, so there is no race for the exit (A11).
            builder.purge_activation_readiness_landed(class_id)?;
            if let Some(mut pool) = builder.state.activation_pools.get(class_id).cloned()
                && (pool.prep_sompi > 0 || pool.bonus_sompi > 0)
            {
                pool.withheld_sompi = pool
                    .withheld_sompi
                    .checked_add(pool.prep_sompi)
                    .and_then(|w| w.checked_add(pool.bonus_sompi))
                    .ok_or(PalwStateV2Error::Overflow("activation pool withheld"))?;
                pool.prep_sompi = 0;
                pool.bonus_sompi = 0;
                builder.write_activation_pool(*class_id, pool)?;
            }
        }
        PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats } => {
            if seats.is_empty() {
                return Err(PalwStateV2Error::EmptyPanel);
            }
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            let PalwClaimPhaseV2::Provisional = claim.phase else {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "PanelBound" });
            };
            builder.require_paid_consumer_bind_v1(*claim_id, &claim, ctx.daa_score)?;
            // **ADR-0147: the outsider is placed by the DRAW, and the draw is checked where it can
            // be.** A bought class's panel carries its outsider at position zero, and whether the
            // seat there is the one the anchor's lottery names over the network's base-class
            // population is a question only the acceptance layer can answer: it needs the anchor's
            // DAA (for maturity), the panel economy's floor and headroom and the readiness policy,
            // none of which this pure transition holds — the same reason every other draw rule
            // (maturity, capability, weighting, the operator lottery) is checked there and not here.
            // `palw_v2_validate_objects` demands the derived panel exactly, in the derived order, and
            // drops any other, so no chain reaches this arm with a panel the draw did not produce.
            //
            // What the transition DOES own is the licence, and that is where the outsider's answer
            // is required (`palw_licence_names_its_outsider_v1`, on every licensing arm). The rule
            // this replaces asked here whether some seat's identity differed from the registrant's;
            // every field it compared is one the registrant writes, so it priced self-judgement at
            // one payout address per seat and stopped nothing.
            // **2026-09-23 audit C-3 (the liveness half): a panel that cannot post the lock binds
            // NOTHING, and the block that carried the object stays valid.** Below the fence the
            // refusal propagated as the transition's error and the virtual processor disqualified
            // the whole carrier block — one unsigned 0x4b object, correctly derived from public
            // state, for a 10,000-sompi relay fee. Past it the object is inert: the claim stays
            // `Provisional`, a later eligible `PanelBound` may still bind it, and otherwise
            // `window_bind` voids it exactly as if nobody had published a panel at all.
            // **RFC-0016 (dormant)**: a claim waiting in a pending Final queue binds no panel — the object is inert, as above; its
            // verification starts when it leaves the queue (its anchor slot then moves with its bind base).
            if builder.state.rfc0016_claim_queued_v1(claim_id) {
                return Ok(());
            }
            match builder.require_panel_lock_eligible(*claim_id, &claim, seats, ctx.daa_score) {
                Ok(()) => {}
                Err(_) if builder.extras.audit_2026_09_23_active => return Ok(()),
                Err(e) => return Err(e),
            }
            // The 2026-09-25 sweep's V03(1): asked BEFORE the panel record exists, which is what
            // tells a `NoCapablePanel` retry from a redraw.
            let was_ncp_retry = palw_claim_awaits_ncp_retry_v1(&builder.state, builder.params, claim_id, &claim);
            builder.write_panel(*claim_id, Some(PalwPanelStateV2 { anchor: *anchor, seats: seats.clone(), bound_daa: ctx.daa_score }))?;
            palw_tir_public_fold_v1::bind_panel_v1(builder, ctx, *claim_id)?;
            // RFC-0006: a panel drawn per shard writes the claim's per-shard record (the plan frozen, the outsider flag, each
            // seat's share of the work); a flat panel writes nothing. Dormant: no plan exists below `palw_tir_shard_v1`.
            palw_tir_shard_fold_v1::bind_record_v1(builder, ctx, claim_id, &claim, anchor, seats)?;
            // ADR-0124 Decision 3: past the fence the drawn seats go on duty — a duty row, and each
            // seat's exposure reserved for the claim's life. Below it nothing is written.
            if builder.extras.panel_economy_active {
                builder.reserve_seat_duties(*claim_id, &claim, seats, ctx.daa_score)?;
            }
            let mut bound = claim;
            // **A `NoCapablePanel` retry is not the one redraw** (V03(1)): its re-basing only moved
            // the anchor, which the panel record now holds, so the bound claim leaves with
            // `rebound_daa` as it was before the retries — `None` — and keeps its one redraw (a
            // receipt timeout or an unreplayed licence still redraws once, rather than voiding and
            // charging the producer for a panel the network's readiness had delayed) and SR-1's
            // early escrow release.
            if was_ncp_retry {
                bound.rebound_daa = None;
            }
            bound.phase = PalwClaimPhaseV2::PanelBound { bound_daa: ctx.daa_score };
            let class_id = bound.class_id;
            let shape = crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(&bound);
            // Lane PL part D: a first panel past `palw_panel_fast_switch` is swept at `T_fast`, not at the whole window.
            let sweep_window = builder.params.panel_bound_sweep_window_v1(
                &bound,
                ctx.daa_score,
                builder.params.receipt_window_for_claim_v1(&builder.state, &class_id, shape, ctx.daa_score),
            );
            builder.write_claim(*claim_id, Some(bound))?;
            // ADR-0164 F-K (K4's denominator): a claim that reached its anchor with a panel.
            builder.note_breaker_v1(ctx.daa_score, crate::palw_capacity_s567_v1::PalwBreakerEventV1::Bound)?;
            builder.disarm_deadline(*claim_id);
            let deadline = ctx.daa_score.checked_add(sweep_window).ok_or(PalwStateV2Error::Overflow("receipt deadline"))?;
            builder.arm_deadline(deadline, *claim_id)?;
        }
        PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts } => {
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            // **ADR-0124 Decision 2, the supplementary door.** Past the fence a claim already
            // licensed still accepts this object until its receipt deadline, carrying only `Valid`
            // receipts of seats on duty the chain has not credited yet (past `palw_rcore_plus`, on a
            // licence awaiting its replay, also a counted seat's whole-job replay — V3S-01): a seat
            // whose receipt the licensing carrier did not hold carries its own, and "who answered in
            // time" is a fact the chain observes rather than one the assembler chose. No phase moves.
            if builder.extras.panel_economy_active && matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
                builder.credit_supplementary_receipts(*claim_id, &claim, receipts, ctx)?;
            } else {
                let PalwClaimPhaseV2::PanelBound { .. } = claim.phase else {
                    return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "ReceiptLicensed" });
                };
                // ADR-0100 Decision 4: a claim whose panel was drawn per shard licenses by parts only.
                // No plan can exist on a network that never armed the fence, so this never fires there.
                if builder.claim_licenses_by_parts(claim_id) {
                    return Err(PalwStateV2Error::LicensedByParts(*claim_id));
                }
                // ADR-0152 S-SPEC §3.4: past `palw_rcore_plus` the licence is the backed subset's.
                if builder.params.rcore_plus_active_at(ctx.daa_score) {
                    let carried: Vec<_> = receipts.iter().map(|receipt| (receipt.clone(), None)).collect();
                    return builder.license_rcore_v1(*claim_id, &claim, PalwLicenceDoorV1::Quorum, &carried, ctx.daa_score);
                }
                // The panel concluded the data WAS served. A seat that signed `Unavailable` accused
                // the producer of withholding what a quorum of its own panel then verified — and the
                // majority invariant (`2·quorum > seat_count`) is what makes that a contradiction
                // rather than a disagreement: both verdicts cannot reach quorum, so exactly one of
                // them is refuted by the record. It pays what it tried to take: the same `reserved`
                // the producer would have lost.
                if builder.extras.audit_2026_09_23_active
                    && !builder.receipt_set_is_backed(*claim_id, &claim, receipts, ctx.daa_score, PalwLicenceDoorV1::Quorum)
                {
                    return Ok(());
                }
                let verdicts = palw_seat_verdicts_of_v2(receipts);
                // **ADR-0147: the outsider's answer is a condition of the licence, not a vote in it.**
                // `2·quorum > seat_count` is satisfied by four seats of five, so a registrant holding
                // the class's whole population licenses its own claim with the outsider watching —
                // unless the outsider's `Valid` is required. It is, here and on every other licensing
                // arm, through the one function, so no path licenses by a rule another path refuses.
                // A genesis class is never outsider-judged; neither is a claim accepted below the
                // fence (the predicate reads the claim's own `accepted_daa`).
                palw_licence_names_its_outsider_v1(
                    &builder.state,
                    claim_id,
                    &claim,
                    &verdicts,
                    builder.extras.admission_independence_daa,
                )?;
                builder.slash_dissenting_seats(claim_id, &claim, &verdicts, true)?;
                // …and the seats that said nothing while their panel concluded without them (P0-7).
                builder.slash_silent_seats(claim_id, &claim, &verdicts)?;
                // ADR-0124 Decision 2: the seats whose `Valid` receipts this object carries are
                // credited — the licensing object is the first carrier of who answered.
                builder.credit_seat_receipts(*claim_id, receipts, ctx.daa_score)?;
                builder.lock_valid_receipts(*claim_id, &claim, receipts, ctx.daa_score, PalwLicenceDoorV1::Quorum)?;
                // ADR-0152 S-SPEC §3.4: the set's R-core+ record (a V2 receipt attests the full cut).
                let carried: Vec<_> = receipts.iter().map(|receipt| (receipt, None)).collect();
                let staged = builder.staged_licence_v1(*claim_id, &claim, PalwLicenceDoorV1::Quorum, &carried, ctx.daa_score);
                builder.license_claim(*claim_id, staged, ctx.daa_score)?;
            }
        }
        PalwConsensusObjectV2::CourtOpened { session_id, claim: claim_id, challenger_bond, space, space_size, signature: _ } => {
            // **ADR-0103 Decision 1: under the held regime nobody walks the leaf ladder.** Every
            // conviction is a one-move object or a dissection the accusation opens at its named
            // leaf, so the ladder's rounds leave the clock and the bisection is refused by name.
            if builder.extras.held_context_ladder.is_some() {
                return Err(PalwStateV2Error::BisectionRefusedUnderHeldContext(*claim_id));
            }
            if builder.state.court_sessions.contains_key(session_id) {
                return Err(PalwStateV2Error::DuplicateSession(*session_id));
            }
            // **How many disputes may run at once, and why that is a number at all** (audit A M-1).
            //
            // Every dissection move is graded in the block that carries it and counts against the
            // network's single per-block court slot (`palw_court_move_spends_the_slot_v1`), while
            // every party owes its move inside one `turn_deadline_daa` and a missed rung is a
            // CONVICTION, not a delay. So the slot is a rate and the rung window is a budget of
            // it: with more live sessions than the window has slots, an attacker opens sessions on
            // claims of its own, emits one cheap `CourtAttnChildChosen` a block, and honest movers
            // are dropped at the acceptance layer until their rung lapses — the responder losing
            // its whole stake, the challenger losing `claim.reserved`.
            //
            // Bounding CONCURRENCY rather than adding a second per-block budget is the fix that
            // keeps one answer to "how much court may one block ask for": the slot stays exactly
            // as wide as it is, and this makes the promise it implies — every session reaches it
            // inside its own rung — true by construction. It is a pure state fact, so the fold,
            // the acceptance rehearsal and the sync walk all answer it the same way.
            // Lane g14-court-capacity (audit G-07): past `palw_court_capacity_v1` court admission (the challenger's reservation, the
            // escrow, the deadline-aware queue) replaces the fixed concurrency cap; below it, the cap as it always was.
            let gated_claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            let gated_window =
                builder.state.class_model_court_window_v1(&gated_claim.class_id).unwrap_or_else(|| builder.params.window_court());
            match palw_court_capacity_fold_v1::court_admission_v1(builder, ctx, *claim_id, *challenger_bond, gated_window)? {
                palw_court_capacity_fold_v1::PalwCourtAdmissionV1::Queued => return Ok(()),
                palw_court_capacity_fold_v1::PalwCourtAdmissionV1::Open => {}
                palw_court_capacity_fold_v1::PalwCourtAdmissionV1::Legacy => {
                    let open_sessions = builder.state.court_sessions.len() as u64;
                    let max_sessions = palw_max_concurrent_court_sessions_v1(builder.params.turn_deadline_daa());
                    if open_sessions >= max_sessions {
                        return Err(PalwStateV2Error::CourtSessionsAtCapacity {
                            open: open_sessions,
                            max: max_sessions,
                            turn_deadline_daa: builder.params.turn_deadline_daa(),
                            per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,
                        });
                    }
                }
            }
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "CourtOpened" });
            }
            // **Opening a court is a bonded act, and this only checked that the bond EXISTS**
            // (audit M2-11). A retiring or under-funded bond could open unlimited sessions — each
            // one freezing an honest claim for a whole window and reserving that claim's stake
            // against collateral that may already be leaving. The attempt path demands Active and
            // the registry floor before it will let a bond risk anything; the court is the same
            // question and now asks it the same way.
            let challenger =
                builder.state.bonds.get(challenger_bond).cloned().ok_or(PalwStateV2Error::MissingBond(*challenger_bond))?;
            if !matches!(challenger.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(*challenger_bond));
            }
            if challenger.collateral < builder.params.min_collateral_sompi() {
                return Err(PalwStateV2Error::BondBelowFloor {
                    bond: *challenger_bond,
                    collateral: challenger.collateral,
                    floor: builder.params.min_collateral_sompi(),
                });
            }
            // The class's own window, if it committed one (`palw_model_court_window`); every other class —
            // all of them below that fence — takes `window_court` and the retention bound below exactly as
            // the release before this one did.
            let model_window = builder.state.class_model_court_window_v1(&claim.class_id);
            let session_window = model_window.unwrap_or_else(|| builder.params.window_court());
            let deadline_daa = ctx.daa_score.checked_add(session_window).ok_or(PalwStateV2Error::Overflow("court deadline"))?;
            palw_exec_v2_rewards_v1::require_native_session_window(&builder.state, builder.params, claim_id, deadline_daa)?;
            // **Court-window bound (mainnet audit 2026-09-11 deep fence): a court's whole window must
            // fit inside the producer's retention obligation** — the rule the DA-accusation path
            // already enforces (`DaOutsideRetention`, SA-1/SA-6), mirrored onto the interactive court.
            // Below the fence `CourtOpened` bounded only its own deadline by `window_court`, so a
            // court opened near the end of retention ran past `claim.trace_retention_daa` and could
            // demand material the obligation already let the producer delete — convicting an honest
            // producer who pruned, and leaving no safe finite retention cap (the disk-growth root of
            // the C-05 revert). Past the fence such a court is refused, so the court can never demand
            // what the obligation released and a producer may prune at `trace_retention_daa`.
            // `validate_palw_v2` requires `min_trace_retention_daa >= window_court` whenever this
            // fence is armed, so a court can still open on every fresh claim (the obligation always
            // covers at least one full window from acceptance).
            //
            // **A class that committed its own window (`palw_model_court_window`) may outrun that obligation**
            // — the obligation was sized by the network window — so for it the opening alone must lie inside
            // retention, and the claim's retention is extended to the session's deadline (the producer owes
            // the trace for as long as the court runs). Every other class keeps the bound above, byte for byte.
            let outside_retention = match model_window {
                Some(_) => ctx.daa_score > claim.trace_retention_daa,
                None => deadline_daa > claim.trace_retention_daa,
            };
            if builder.extras.audit_2026_09_11_deep_active && outside_retention {
                return Err(PalwStateV2Error::CourtOutsideRetention {
                    claim: *claim_id,
                    at: ctx.daa_score,
                    window: session_window,
                    retention_daa: claim.trace_retention_daa,
                });
            }
            if model_window.is_some() && deadline_daa > claim.trace_retention_daa {
                let mut extended_claim = claim.clone();
                extended_claim.trace_retention_daa = deadline_daa;
                builder.write_claim(*claim_id, Some(extended_claim))?;
            }
            // **The opening rung is clocked like every other rung — because a responder now ships.**
            //
            // A rung clock convicts on silence: `declare_no_show` reads whose turn it was and ends
            // the session against them. That is only sound when the silent party COULD have moved,
            // and this rung once had no mover: `CourtDisclosed` was constructed nowhere in this
            // tree, so a tight clock let anyone holding one bond convict any producer by opening a
            // court and waiting — against a producer whose only fault was that no responder
            // software existed. The rung was therefore given the whole session budget as a
            // backstop, and its lapse ended the dispute on the CHALLENGER's side.
            //
            // The panel service now emits `CourtDisclosed` (`palw_panel.rs`, the disclosure arm),
            // and a close now carries the operand openings the court needs to read, so a producer
            // that answers is a producer the chain can adjudicate end to end. With a mover in the
            // field the backstop inverts: it made SILENCE the winning move for a guilty producer,
            // which is the one outcome a fraud court may not have. So the opening rung takes
            // `turn_deadline_daa` like the rest of the ladder.
            //
            // **What this asks of a bundle.** `turn_deadline_daa` is now a real deadline for the
            // producer's first move, so it has to be wider than the responder's own cadence: the
            // panel wakes on a 2-second tick and answers from a capture it already holds, so on a
            // one-block-per-second network a window of a handful of DAA is a coin flip and one of
            // a few dozen is comfortable. No shipped preset carries `ConsensusV2` — the bundle
            // arrives with a network's genesis — so this is a genesis-time choice, and a bundle
            // whose window is tighter than its own software convicts honest producers.
            //
            // **The deployment gate is the ruleset, not a flag.** This changes what silence costs,
            // so it may not reach a network whose producers are running a build with no responder
            // in it — and it does not: `palw_ruleset_id_v2` covers these params, and the networks
            // that adopt it are the ones re-minted from this build.
            let first_deadline_daa = ctx
                .daa_score
                .checked_add(builder.params.turn_deadline_daa())
                .ok_or(PalwStateV2Error::Overflow("court opening rung deadline"))?
                .min(deadline_daa);
            // The ladder's id is derived from the same six inputs `court_session_id_v2` uses, so
            // a ladder whose id is not `session_id` means the object's declared space does not
            // match the id it was opened under — refused here, not merely at the acceptance
            // layer, because the transition is what every node recomputes.
            let ladder = crate::palw_bisect::PalwBisectLadderV1::open(
                claim_id,
                &claim.trace_root,
                &crate::palw_court_v2::court_party_id_v2(challenger_bond),
                &crate::palw_court_v2::court_party_id_v2(&claim.bond),
                *space,
                *space_size,
                ctx.daa_score,
                first_deadline_daa,
            )
            .map_err(|e| PalwStateV2Error::LadderRefused(*session_id, e.to_string()))?;
            if ladder.session_id() != *session_id {
                return Err(PalwStateV2Error::SessionIdMismatch(*session_id));
            }
            // **An accusation costs the accuser what it puts at risk for the accused.**
            //
            // Opening used to cost one transaction fee: nothing was reserved, `Active` was not
            // required, and every challenger-losing exit went through
            // `rearm_after_challenger_side_close`, which slashes nobody. Meanwhile the responder's
            // silence was priced at its whole claim. One bond could therefore hold every producer
            // on the network under permanent accusation for the price of postage.
            //
            // The reservation is the claim's own `reserved` — the same number the executor has at
            // stake in the dispute, so the two sides face the same figure — and it is charged to
            // the challenger's exposure.
            //
            // **That charge was not counted against anything** (ADR-0062 SA-7(c), second arm).
            // This comment used to end "the ceiling it already lives under does the counting"; it
            // does not, and SA-7's own commit message says why in the accusation's margin — the
            // only exposure ceiling in the tree is applied in `check_palw_attempt_admission_v2`,
            // i.e. only to a bond that wants to PRODUCE. Reserving does not touch collateral, so
            // `Active` and the registry floor are checks one bond passes once per live claim: it
            // could open a court on all of them in a single block, freeze every one of them until
            // its `window_court` backstop, and pay for at most the first close (the challenger-side
            // charge is `min(reserved, floor)` and `slash_bond` clamps at collateral and returns
            // early at zero). The accusation arm's ceiling is this arm's ceiling now — same
            // helper, same ledger, same figure.
            //
            // **Fenced on `palw_da_court`**, which is this ADR's fence and therefore the one this
            // widens rather than a second one: with the fence dormant the arm reserves exactly what
            // it always reserved, which is what `the_da_court_fence_off_is_byte_identical` pins.
            // The consequence is that the gap is closed for a network that arms the DA court and
            // stays open for one that does not — named here rather than left for a reader to
            // discover, because arming is a genesis-time choice somebody has to make knowing that.
            let challenger_collateral = challenger.collateral;
            builder.reserve_challenger_v1(*challenger_bond, challenger_collateral, claim.reserved, "court opening", ctx.daa_score)?;
            let mut opened = PalwCourtSessionStateV2 {
                claim: *claim_id,
                challenger_bond: *challenger_bond,
                opened_daa: ctx.daa_score,
                deadline_daa,
                ladder,
                // ADR-0082 Decision 2: a session opens with no dissection. The phase is entered at
                // the ladder's `Terminal`, by the responder's root claim, and only under
                // `Params::palw_kary_court`.
                dissection: None,
            };
            // ADR-0080 W4 item 4: the opening rung is a rung, so it takes the same cap — before
            // the record is written, so the deadline index is built from the capped value and one
            // write is one write.
            cap_session_rung_deadline_v2(&mut opened, builder.params);
            builder.write_court(*session_id, Some(opened))?;
            palw_court_capacity_fold_v1::court_opened_v1(builder, ctx, *claim_id, *challenger_bond, *session_id)?;
            // An open court freezes the path to Final: the claim keeps no deadline while any
            // session is open (void-by-timeout of the COURT is PR-07's deadline system).
            //
            // **`DefaultDisputed` is deliberately not in this set** (ADR-0062 SA-7). Past the DA
            // fence a claim can be challengeable while disputed — `validate_court_opened_v2` reads
            // the challenge surface through the accusation — and the deadline such a claim holds is
            // the DISCLOSE deadline, not a path to `Final`. Disarming it would delete the silence
            // clock the accusation runs on, and the DA sweep would never fire: opening a court
            // would then be the way to cancel an accusation. The disclosure re-arms through
            // `rearm_claim_after_da_session`, which asks `open_courts_by_claim` the same question
            // this does, so the two cannot arm a final deadline behind each other's back.
            if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
                builder.disarm_deadline(*claim_id);
            }
        }
        PalwConsensusObjectV2::CourtClosed { session_id, verdict, proof } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            builder.write_court(*session_id, None)?;
            let claim_id = session.claim;
            let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
            match verdict {
                PalwCourtVerdictV2::ExecutorGuilty => {
                    // A verdict against an already-terminal claim closes the session and changes
                    // nothing else: a late verdict must not brick the block that carries it, and
                    // Final-vs-guilty contradictions are the deep-reorg surface PR-07/PR-08 own.
                    if !claim.phase.is_terminal() {
                        // A proven arithmetic fault is the one conviction the ruleset can make
                        // from evidence alone, so it is the one that most obviously must cost
                        // the executor its stake (audit C5: it did not). C-03 (deep fence): and the
                        // convicted responder pays for the time it kept the court open. S-4: past
                        // `palw_rcore_plus` with S2's action and the `CourtConviction` record, the
                        // challenger the reward's winner.
                        //
                        // ADR-0152 §4-ter (the review's F3, decision (B)): a dissection's bottom proves
                        // the producer's own disclosure false, not the execution the seats replayed —
                        // whatever the class — recorded `CourtHeldVerdict`, charged the same, no
                        // kind-3 basis.
                        // RFC-0002 F7: an IR dissection's bottom proves the same thing — the responder's
                        // own claim of partials false — so it is recorded the same way. RFC-0003: and a
                        // pipeline's (`GenDissection`, F7 composed into the generative court) — the arm
                        // listed the first two and left the third a `CourtFraud`, which kind 3 reads as
                        // the replayed execution proven false against every full-mask `Valid` signer
                        // (lane B's finding, 2026-10-01).
                        let reason = palw_court_verdict_void_reason_v1(
                            builder.extras.offence_attribution_active,
                            proof.is_dissection_bottom_v1(),
                        );
                        builder.convict_by_court_verdict_as_v1(
                            ctx,
                            claim_id,
                            &claim,
                            session.challenger_bond,
                            Some(session.opened_daa),
                            reason,
                        )?;
                    }
                }
                PalwCourtVerdictV2::ChallengerDefeated => {
                    let before = builder.state.bonds.get(&session.challenger_bond).map(|record| record.collateral).unwrap_or(0);
                    rearm_after_challenger_side_close(builder, ctx, claim_id, &claim, session.challenger_bond, session.opened_daa)?;
                    // ADR-0152 §4-ter.3 step 6 (the forger's race): past `palw_offence_attribution`, what
                    // a proven acquittal on an anchor took, kept for the conviction that proves it wrong.
                    builder.record_held_forfeit_v1(
                        ctx,
                        claim_id,
                        *session_id,
                        session.challenger_bond,
                        session.ladder.terminal_index(),
                        before,
                        proof,
                    )?;
                }
            }
        }
        PalwConsensusObjectV2::CourtDisclosed { session_id, disclosure, signature: _ } => {
            let mut session =
                builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            session
                .ladder
                .apply_disclosure(disclosure, ctx.daa_score, builder.params.turn_deadline_daa)
                .map_err(|e| PalwStateV2Error::LadderRefused(*session_id, e.to_string()))?;
            cap_session_rung_deadline_v2(&mut session, builder.params);
            builder.write_court(*session_id, Some(session))?;
        }
        PalwConsensusObjectV2::CourtVerdictPosted { session_id, verdict, signature: _ } => {
            let mut session =
                builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            session
                .ladder
                .apply_verdict(verdict, ctx.daa_score, builder.params.turn_deadline_daa)
                .map_err(|e| PalwStateV2Error::LadderRefused(*session_id, e.to_string()))?;
            cap_session_rung_deadline_v2(&mut session, builder.params);
            builder.write_court(*session_id, Some(session))?;
        }
        PalwConsensusObjectV2::CourtCloseDeclared { session_id, side, count, chunk_digests, close_digest, verdict, signature: _ } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            let claim = builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();
            // **The declarer is READ from the two bonds the session id already binds** — the
            // challenger from the session record, the executor from the claim — so a declaration
            // can never name a third party and the row can never be squatted. Whether the
            // signature is that bond's is the acceptance layer's, where the registry is in hand:
            // the same split `CourtDisclosed` and `CourtVerdictPosted` already use.
            let declarer = match side {
                PalwCourtSideV1::Challenger => session.challenger_bond,
                PalwCourtSideV1::Executor => claim.bond,
            };
            if !builder.state.bonds.contains_key(&declarer) {
                return Err(PalwStateV2Error::CourtCloseSideNotBound { session: *session_id });
            }
            // **A close is legal only at `Terminal`, and until audit H-2 nothing said so** — the
            // sentence was written in this arm's own comment ten lines below and enforced nowhere,
            // so the row could be opened at round 0.
            //
            // Two things follow from opening early, and both are the declarer's free option. The
            // row is priced for `4 x count` DAA of assembly and is deleted only when the SESSION
            // ends, so a declaration at round 0 holds up to 2.7 MB inside `collection_root` — and
            // therefore inside every `state_root` and every tip snapshot — for the whole
            // `window_court` while the ladder plays out beneath it. And the deposit it risks is
            // collected only where the group ends without the close it pinned, which a declarer
            // that never files the last chunk can arrange to happen after everything it cares
            // about is already decided.
            //
            // The turn is read through `court_turn_and_rung_deadline_v2`, the same helper
            // `court_next_deadline_v2` reads it through, so the dissection phase answers for a
            // session that has one and the ladder answers for a session that does not: one
            // spelling of "whose move is it", not two.
            // A fused class with no phase open is not at `Terminal` for this purpose — its
            // terminal move is the responder's root claim (ADR-0082 C-5), which is exactly the
            // move a close would be declared INSTEAD of. The same helper answers both questions.
            //
            // **RFC-0002 F7: except the IR executor's own terminal move.** At an IR class's terminal
            // the executor is clocked at every leaf, and at a leaf that is not dissected its move is
            // an acquitting close — declared, when it does not fit one carrier. Refusing the
            // declaration there would leave an honest executor no move its clock accepts; allowing
            // it costs nothing it should not: the declared close must deliver AND adjudicate, or its
            // declarer is convicted at its own assembly deadline (a whole close at a dissected leaf
            // that does not fit the court is such a failure).
            let turn = court_session_turn_and_rung_deadline_v2(&builder.state, &session).0;
            let the_ir_executor_s_move =
                *side == PalwCourtSideV1::Executor && court_session_at_the_ir_terminal_v1(&builder.state, &session);
            if turn != crate::palw_bisect::PalwBisectTurnV1::Terminal && !the_ir_executor_s_move {
                return Err(PalwStateV2Error::CourtCloseNotTerminal { session: *session_id, turn });
            }
            // One declaration per `(session, side)`, ever. It is what makes the interlock's
            // single-assembly term true, and what stops either party buying rung after rung of
            // free window by re-declaring.
            if builder.state.court_close_groups.contains_key(&(*session_id, *side)) {
                return Err(PalwStateV2Error::DuplicateCourtCloseGroup { session: *session_id });
            }
            if *count == 0 || *count > PALW_COURT_CLOSE_MAX_CHUNKS {
                return Err(PalwStateV2Error::CourtCloseCountOutOfRange { count: *count, max: PALW_COURT_CLOSE_MAX_CHUNKS });
            }
            if chunk_digests.len() != *count as usize {
                return Err(PalwStateV2Error::CourtCloseDigestsIncoherent { session: *session_id });
            }
            // **The declaration's clock is the BACKSTOP, and the backstop never moves.**
            //
            // A close is legal only at `Terminal`, and `court_next_deadline_v2` returns
            // `session.deadline_daa` alone there — "Terminal has no move the responder can make"
            // (audit M2-4). So the only clock that can still end this session is the whole-session
            // backstop, which closes on the CHALLENGER's side. A declaration that cannot assemble
            // inside it is therefore not a slow close, it is a rung suspended for nothing, and
            // extending the backstop to fit it would hand either party a free window for the price
            // of a declaration. Refused here instead, at the block that carries it, where the
            // mover can still make a smaller one.
            let needed = ctx
                .daa_score
                .checked_add(palw_close_assembly_daa_v1(*count))
                .ok_or(PalwStateV2Error::Overflow("close assembly deadline"))?;
            if needed > session.deadline_daa {
                return Err(PalwStateV2Error::CourtCloseCannotAssemble {
                    session: *session_id,
                    count: *count,
                    needed,
                    deadline: session.deadline_daa,
                });
            }
            // **W10: the assembly reserve, written from the ruleset and backed by a bond the chain
            // can actually charge.**
            //
            // ADR-0075 SA-1 could state this rule and not build it — "a refund needs the chain to
            // REMEMBER which outpoint backs which group, which is a field on `PalwPendingChunksV2`
            // and therefore a schema change" — and W5's state-version bump is what afforded the
            // fields. The outpoint is the DECLARER'S OWN BOND, not a fresh UTXO the object names:
            // a bond key IS a `TransactionOutpoint`, the declaration is already signed by that
            // bond's key, and the collateral behind it is already locked and already slashable, so
            // the deposit needs no new lock and cannot be spent out from under the group.
            //
            // **A bond too poor to cover it is NOT refused here**, and that is a decision rather
            // than an omission. Refusing would make a bond that has already been slashed towards
            // zero undeclarable, which on the executor's side is the accused losing its widest
            // defence for being poor — a free win for whichever challenger drained it first. The
            // deposit is therefore a CHARGE, collected by `slash_bond` against whatever collateral
            // is still there, which is exactly what every other punishment in this ruleset is
            // (`void_and_slash`, `slash_seat`, the DA burn all clamp the same way). It is as
            // enforceable as they are and no more.
            let deposit = palw_close_assembly_deposit_v1(*count);
            builder.write_court_close_group(
                (*session_id, *side),
                Some(PalwCourtCloseGroupV2 {
                    declarer,
                    count: *count,
                    chunk_digests: chunk_digests.clone(),
                    close_digest: *close_digest,
                    verdict: *verdict,
                    declared_daa: ctx.daa_score,
                    assembly_deadline_daa: needed,
                    present: 0,
                    deposit_outpoint: declarer.0,
                    deposit,
                    chunks: BTreeMap::new(),
                }),
            )?;
        }
        PalwConsensusObjectV2::SeatReadinessProved { bond, class_id, span, opening, signature: _ } => {
            // ADR-0133 §11.2: past readiness V2 the one-leaf proof is not evidence any more.
            if builder.extras.readiness_v2_active {
                return Err(PalwStateV2Error::ReadinessV1Superseded(*class_id));
            }
            check_readiness_model_scope_v1(builder, ctx, class_id)?;
            builder.apply_seat_readiness(ctx, bond, class_id, *span, opening)?;
        }
        PalwConsensusObjectV2::SeatReadinessProvedV2 { bond, class_id, span, proof, signature: _ } => {
            check_readiness_model_scope_v1(builder, ctx, class_id)?;
            builder.apply_seat_readiness_v2(ctx, bond, class_id, *span, proof)?;
        }
        PalwConsensusObjectV2::ObjectiveOffence { kind, accused, evidence_id, evidence } => {
            builder.consume_objective_offence(ctx, *kind, *accused, *evidence_id, evidence)?;
        }
        PalwConsensusObjectV2::ClassManifestV2 { class_id, artifact_bytes, registrant_bond, signature: _ } => {
            builder.apply_class_manifest(ctx, class_id, *artifact_bytes, registrant_bond)?;
        }
        PalwConsensusObjectV2::ReceiptLicensedV2 { claim: claim_id, receipts } => {
            apply_receipt_licensed_v2(builder, ctx, claim_id, receipts)?;
        }
        // **ADR-0160 F-B: the batch licence** (tag 59). Past `Params::palw_capacity_batch_licence`
        // only (the state params' mirror); refused by name below it, as the acceptance layer refuses
        // it. The acceptance layer verified each root's signature once and each live entry's paths
        // and coverage through the single object's own validator
        // ([`crate::palw_batch_licence_v1::palw_validate_batch_licence_v1`]); the fold routes each
        // entry by the one predicate acceptance used, on the state it folds, and feeds a live one's
        // receipts to the single object's arm — no licensing rule is restated (ADR-0160 V-I1).
        PalwConsensusObjectV2::ReceiptLicensedBatchV1 { roots: _, entries } => {
            if !builder.params.capacity_batch_active_at(ctx.daa_score) {
                return Err(PalwStateV2Error::CapacityBatchLicenceDormant);
            }
            for entry in entries {
                let route =
                    crate::palw_batch_licence_v1::palw_batch_entry_route_v1(&builder.state, builder.params, ctx.daa_score, entry);
                if route == crate::palw_batch_licence_v1::PalwBatchEntryRouteV1::Inert {
                    continue;
                }
                // A claim another object licensed first feeds only the seats the chain has not
                // counted (the lane-verify review's batch finding) — acceptance judged exactly these.
                let live = crate::palw_batch_licence_v1::palw_batch_entry_live_v1(&builder.state, route, entry);
                let seats: Vec<PalwBondKeyV2> = builder
                    .state
                    .panels
                    .get(&entry.claim)
                    .map(|panel| panel.seats.iter().map(|seat| seat.bond).collect())
                    .unwrap_or_default();
                let receipts = crate::palw_batch_licence_v1::palw_batch_entry_receipts_v1(&live, &seats)
                    .ok_or(PalwStateV2Error::WrongPhase { claim: entry.claim, edge: "ReceiptLicensedBatchV1" })?;
                apply_receipt_licensed_v2(builder, ctx, &entry.claim, &receipts)?;
            }
        }
        // **ADR-0160 F-Q: an audit receipt batch** (tag 60), past `Params::palw_capacity_audit_door`
        // only (the mirror). The acceptance layer verified its signature; the fold refuses what makes it
        // invalid and skips the entries it does not keep (`palw_audit_door_v1`).
        PalwConsensusObjectV2::AuditReceiptBatchV1 { auditor, entries, signature: _ } => {
            if !builder.params.capacity_audit_active_at(ctx.daa_score) {
                return Err(PalwStateV2Error::CapacityAuditDoorDormant);
            }
            apply_audit_receipt_batch_v1(builder, ctx, auditor, entries)?;
        }
        // **ADR-0164 F-M1: riders (tag 95)**, past `Params::palw_capacity_multi_claim` only. The acceptance layer verified every rider's
        // signature; the fold only QUEUES the object here — the lead it names may be created by this very block's work (a merged
        // blue), so the batch is applied at step 4b′, after the work.
        PalwConsensusObjectV2::AttemptRidersV1 { lead, riders } => {
            if !builder.params.capacity_riders_active_at(ctx.daa_score) {
                return Err(PalwStateV2Error::CapacityRidersDormant);
            }
            builder.pending_riders.push((*lead, riders.clone()));
        }
        // **ADR-0152 v3.1 R-3 (S-7): a reporter's commitment (tag 53).** Past
        // `Params::palw_rcore_plus` only; below it (every network but testnet-12, and testnet-12
        // with the fence off) the skeleton's refusal by name stands, byte for byte.
        PalwConsensusObjectV2::ReporterCommitted { commitment, reporter, signature: _ } => {
            if !builder.params.rcore_plus_active_at(ctx.daa_score) {
                return Err(PalwStateV2Error::RcoreObjectNotLanded(palw_rcore_object_name_v1(object).unwrap_or("an R-core+ object")));
            }
            apply_reporter_committed(builder, ctx, *commitment, *reporter)?;
        }
        // **ADR-0152 v3.1 R-3/R-4 (S-7): a reveal (tag 54)**, under the same fence.
        PalwConsensusObjectV2::ReporterRevealed { offence_key, reporter, salt } => {
            if !builder.params.rcore_plus_active_at(ctx.daa_score) {
                return Err(PalwStateV2Error::RcoreObjectNotLanded(palw_rcore_object_name_v1(object).unwrap_or("an R-core+ object")));
            }
            apply_reporter_revealed(builder, ctx, *offence_key, *reporter, salt)?;
        }
        // M3 (DA-4): tag 55 past `palw_rcore_plus` — any locked signer's answer to a DA session.
        PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, discloser, signature: _ }
            if builder.params.rcore_plus_active_at(ctx.daa_score) =>
        {
            return apply_da_answer_v1(builder, ctx, *claim, *unit, answer, *discloser);
        }
        // **ADR-0152 v3.1 §6, the v22 skeleton: tag 55 below the fence and tag 56 are declared, not
        // landed.** Refused by name — `Params::palw_rcore_plus` arms 55 (M3, above) and not yet 56
        // (S-5). The processor's gate drops them first, so a block carrying one stands and folds
        // nothing for it.
        PalwConsensusObjectV2::MaterialDisclosedV2 { .. } | PalwConsensusObjectV2::PanelUnavailableQuorum { .. } => {
            return Err(PalwStateV2Error::RcoreObjectNotLanded(palw_rcore_object_name_v1(object).unwrap_or("an R-core+ object")));
        }
        PalwConsensusObjectV2::OptimisticLicensed { claim: claim_id, receipts } => {
            if !builder.extras.verification_s2_active {
                return Err(PalwStateV2Error::VerificationV2Dormant);
            }
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            let PalwClaimPhaseV2::PanelBound { .. } = claim.phase else {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "OptimisticLicensed" });
            };
            if builder.claim_licenses_by_parts(claim_id) {
                return Err(PalwStateV2Error::LicensedByParts(*claim_id));
            }
            // ADR-0152 S-SPEC §3.4: past `palw_rcore_plus` the licence is the backed subset's (the full
            // seat locks `lock_2` like every counted signer; `lock_1` is gone).
            if builder.params.rcore_plus_active_at(ctx.daa_score) {
                let carried: Vec<_> = receipts.iter().map(|r| (r.receipt.clone(), Some(r.segments))).collect();
                return builder.license_rcore_v1(*claim_id, &claim, PalwLicenceDoorV1::Optimistic, &carried, ctx.daa_score);
            }
            let inner: Vec<crate::palw_panel_v2::PalwSeatReceiptV2> = receipts.iter().map(|r| r.receipt.clone()).collect();
            // Audit #6: the full-replay seat is the whole colluding set of this door, so it must be
            // able to post the whole gain; if it cannot, the set is inert and the other doors stay open.
            if builder.extras.audit_2026_09_23_active
                && !builder.receipt_set_is_backed(*claim_id, &claim, &inner, ctx.daa_score, PalwLicenceDoorV1::Optimistic)
            {
                return Ok(());
            }
            let verdicts = palw_seat_verdicts_of_v2(&inner);
            palw_licence_names_its_outsider_v1(
                &builder.state,
                claim_id,
                &claim,
                &verdicts,
                builder.extras.admission_independence_daa,
            )?;
            builder.slash_dissenting_seats(claim_id, &claim, &verdicts, true)?;
            builder.slash_silent_seats(claim_id, &claim, &verdicts)?;
            builder.credit_seat_receipts(*claim_id, &inner, ctx.daa_score)?;
            builder.lock_valid_receipts(*claim_id, &claim, &inner, ctx.daa_score, PalwLicenceDoorV1::Optimistic)?;
            // ADR-0152 S-SPEC §3.4: an S2 set records its door and recount (Q-3: 1 for the full seat
            // and one partial), and S2 never releases the escrow (SR-1 cond. 1).
            let carried: Vec<_> = receipts.iter().map(|r| (&r.receipt, Some(r.segments))).collect();
            let staged = builder.staged_licence_v1(*claim_id, &claim, PalwLicenceDoorV1::Optimistic, &carried, ctx.daa_score);
            builder.license_claim(*claim_id, staged, ctx.daa_score)?;
        }
        PalwConsensusObjectV2::CourtCloseChunk { session_id, side, index, bytes } => {
            let mut group = builder
                .state
                .court_close_groups
                .get(&(*session_id, *side))
                .ok_or(PalwStateV2Error::MissingCourtCloseGroup { session: *session_id })?
                .clone();
            if ctx.daa_score > group.assembly_deadline_daa {
                return Err(PalwStateV2Error::CourtCloseGroupExpired { session: *session_id });
            }
            if *index >= group.count {
                return Err(PalwStateV2Error::CourtCloseIndexOutOfRange { index: *index, count: group.count });
            }
            if group.has(*index) {
                return Err(PalwStateV2Error::CourtCloseChunkDuplicate { session: *session_id, index: *index });
            }
            if bytes.is_empty() || bytes.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES {
                return Err(PalwStateV2Error::CourtCloseChunkTooLarge { bytes: bytes.len(), max: PALW_COURT_CLOSE_CHUNK_MAX_BYTES });
            }
            // **Refused at ARRIVAL, not at assembly.** The declaration already pinned this index's
            // bytes, so a chunk is either the pinned preimage or it is nothing — strictly earlier
            // than ADR-0075 D14, where a substituted part is only discovered at
            // `ChunkGroupHashMismatch` after the table has carried it for the whole TTL.
            if palw_court_close_chunk_digest_v1(bytes) != group.chunk_digests[*index as usize] {
                return Err(PalwStateV2Error::CourtCloseChunkDigestMismatch { session: *session_id, index: *index });
            }
            group.present |= 1u64 << index;
            group.chunks.insert(*index, bytes.clone());
            if !group.is_complete() {
                // O(1) in the group (audit H-3): the arrival is journalled as an arrival, not as a
                // swap of the whole accumulated record. `group` here is the local clone the checks
                // above were made against; the writer applies the same bit and bytes to the row in
                // state, which is why nothing but the delta shape changes.
                builder.write_court_close_chunk((*session_id, *side), *index, bytes.clone())?;
                return Ok(());
            }
            // **W7: the chunk that COMPLETES a group assembles it, and the close happens here.**
            //
            // Not written back first: every path below removes the session, and `write_court`
            // removes its groups with it, so writing the completed row would be a state entry
            // whose only reader is the delete two lines later.
            //
            // Three questions, in the order that makes an expensive one unreachable behind a cheap
            // one, and each of them settled against something the DECLARER signed:
            //
            //  1. do the pinned parts, concatenated, hash to the `close_digest` the declaration
            //     pinned? This is the belt to the per-chunk braces: every chunk was checked at
            //     arrival, so a group can be full of individually-correct parts of an inconsistent
            //     whole, and only this catches that.
            //  2. do those bytes decode to a `CourtClosed` for THIS session carrying the verdict
            //     the declaration announced? A declaration that pins bytes decoding to something
            //     else — or to a close of a different session — has pinned an object no court can
            //     act on.
            //  3. does the proof inside actually adjudicate to that verdict? That question needs
            //     `PalwCourtParamsV2`, which the transition does not have and deliberately does not
            //     take: it is the acceptance layer's, exactly as it is for a single-carrier
            //     `CourtClosed`, whose arm below also acts on a verdict this fold never re-derives.
            //     `palw_v2_validate_objects` refuses a completing chunk whose assembly does not
            //     adjudicate to the declaration's verdict, so nothing reaches here that a
            //     one-carrier close could not have reached.
            //
            // 1 and 2 are answered HERE and not there, and the direction is the reason: a refused
            // object is DROPPED with the block standing, so refusing at acceptance would leave the
            // group intact and let the declarer sit on a reserve it will never assemble until the
            // backstop. Its own bytes not decoding is a fact any node establishes from rooted state
            // alone, it is entirely the declarer's doing, and W5 established which way it falls —
            // "a failed declaration loses on its OWN side". So it is a conviction in the block that
            // carries the last chunk, not a wait for the sweep.
            let mut assembled = Vec::with_capacity(group.chunks.values().map(Vec::len).sum());
            for index in 0..group.count {
                assembled.extend_from_slice(group.chunks.get(&index).expect("a complete group holds every index below count"));
            }
            let decoded = (palw_court_close_chunk_digest_v1(&assembled) == group.close_digest)
                .then(|| borsh::from_slice::<PalwConsensusObjectV2>(&assembled).ok())
                .flatten()
                .filter(|object| {
                    matches!(object, PalwConsensusObjectV2::CourtClosed { session_id: decoded_session, verdict, .. }
                        if decoded_session == session_id && *verdict == group.verdict)
                })
                // RFC-0002 Phase F: below `palw_tir_v1` an IR proof is not in the enum an older build
                // decodes with, so its bytes do not decode there — and do not decode here either.
                .filter(|object| {
                    !matches!(object, PalwConsensusObjectV2::CourtClosed { proof, .. }
                        if proof.is_tir_v1() && !builder.params.tir_active_at(ctx.daa_score))
                })
                // RFC-0003: likewise a generative proof below `palw_gen_v1`.
                .filter(|object| {
                    !matches!(object, PalwConsensusObjectV2::CourtClosed { proof, .. }
                        if proof.is_gen_v1() && !builder.params.gen_active_at(ctx.daa_score))
                })
                // The task-head profile: likewise a proof carrying a `Head` job below `palw_task_heads_v1`.
                .filter(|object| {
                    !crate::palw_task_heads_v1::palw_object_needs_task_heads_v1(object)
                        || builder.params.task_heads_active_at(ctx.daa_score)
                })
                // RFC-0004 A6: likewise an evaluation proof below `palw_improvement_v1`.
                .filter(|object| {
                    !matches!(object, PalwConsensusObjectV2::CourtClosed { proof, .. }
                        if proof.is_eval_v1() && !builder.params.improve_active_at(ctx.daa_score))
                })
                // ADR-0096 Decision 7: likewise a constrained decode proof below `palw_fp_decode_constraint`.
                .filter(|object| {
                    !matches!(object, PalwConsensusObjectV2::CourtClosed { proof, .. }
                        if proof.is_constrained_decode_v1() && !builder.params.fp_decode_constraint_active_at(ctx.daa_score))
                });
            let Some(close) = decoded else {
                return convict_close_declarer_v1(builder, ctx, *session_id, *side);
            };
            // **The row DELIVERED, and it has to say so before the close ends the session** (audit
            // H-2b). `write_court` collects the assembly deposit of every group still standing at
            // a session's removal, and the `CourtClosed` applied on the next line removes this
            // one — so an honest assembly that stayed in the table would be charged for the row it
            // just filed. This is the single ending that owes nothing.
            builder.end_close_group_delivered((*session_id, *side))?;
            // **Through the arm, never beside it.** The decoded object is a `CourtClosed` and is
            // applied by re-entering this function, so a split close and a whole close are the same
            // state machine by construction rather than by two authors agreeing: a change to the
            // `CourtClosed` arm is a change to this path in the same edit. It terminates because
            // the filter above admits only `CourtClosed`, which reaches no arm that recurses.
            apply_object(builder, ctx, &close)?;
        }
        // -------------------------------------------------------------------------------------
        // ADR-0082 Decision 2 — the dissection's three moves
        //
        // Each arm does exactly two things: it refuses a move that is not legal in this session,
        // and it hands the message to `PalwAttnDissectPhaseV1`, whose rules are NOT restated here.
        // The out-of-turn refusal, the fold that must check, the child that must be in range and
        // the round budget are all the phase's; this is the wiring, and a second copy of a rule is
        // a second rule.
        //
        // **The admissibility fence is the acceptance layer's** (`palw_v2_validate_objects` under
        // `Params::palw_kary_court_active_at`), the same split every court object's SIGNATURE
        // uses. What the fold enforces on its own is the arity: an illegal one is refused here,
        // and whether the legal one is the ruleset's derived value is checked where the bundle is.
        // -------------------------------------------------------------------------------------
        PalwConsensusObjectV2::CourtAttnRootClaimed { .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedAnchored { .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedHeld { .. } => {
            // **ADR-0093 Decision 8: move 1 has two forms, and past its fence the one without the
            // anchor is refused wherever the bottom will read a checkpoint.** Both open the same
            // phase; the anchored form's anchor is checked against the site the class derives and
            // then left on the chain for the bottom to read — the phase keeps nothing new.
            //
            // **ADR-0152 §4-ter C2: and a third, the held form (tag 57)** — the anchored form plus
            // the anchor's slice sub-roots, checked against the anchor's committed state root
            // (H2/H3 below). Past `palw_offence_attribution` it is the only form an answerable held
            // class may file where the bottom reads a checkpoint; it too opens the same phase.
            let (session_id, root, arity, binding, out_tile, operand_openings, filed_anchor, held_sub_roots) = match object {
                PalwConsensusObjectV2::CourtAttnRootClaimed {
                    session_id, root, arity, binding, out_tile, operand_openings, ..
                } => (session_id, root, arity, binding, out_tile, operand_openings, None, None),
                PalwConsensusObjectV2::CourtAttnRootClaimedAnchored {
                    session_id,
                    root,
                    arity,
                    binding,
                    out_tile,
                    anchor,
                    operand_openings,
                    ..
                } => (session_id, root, arity, binding, out_tile, operand_openings, Some(anchor.as_ref()), None),
                PalwConsensusObjectV2::CourtAttnRootClaimedHeld {
                    session_id,
                    root,
                    arity,
                    binding,
                    out_tile,
                    anchor,
                    slice_sub_roots,
                    operand_openings,
                    ..
                } => (
                    session_id,
                    root,
                    arity,
                    binding,
                    out_tile,
                    operand_openings,
                    Some(anchor.as_ref()),
                    Some(slice_sub_roots.as_slice()),
                ),
                _ => unreachable!("the arm matched one of the three"),
            };
            if held_sub_roots.is_some() && !builder.extras.offence_attribution_active {
                return Err(PalwStateV2Error::HeldRootClaimRefused {
                    session: *session_id,
                    why: "a held root claim before palw_offence_attribution is armed (ADR-0152 §4-ter)",
                });
            }
            if filed_anchor.is_some() && held_sub_roots.is_none() && !builder.extras.attn_anchored_root_active {
                return Err(PalwStateV2Error::DissectionRefused(
                    *session_id,
                    "an anchored root claim before palw_attn_anchored_root is armed (ADR-0093 Decision 8)".to_string(),
                ));
            }
            let mut session =
                builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            if session.dissection.is_some() {
                return Err(PalwStateV2Error::DissectionAlreadyOpen(*session_id));
            }
            // **A dissection begins where the ladder ends.** The phase adjudicates ONE leaf, and
            // which leaf that is is the session's own narrowing — so a root claim before the
            // ladder is terminal is a claim about a step nobody chose.
            let narrowed = session.ladder.terminal_index().ok_or(PalwStateV2Error::LadderNotTerminal(*session_id))?;
            let claim = builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();
            let class = builder.state.classes.get(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.clone();
            // **ADR-0152 §4-ter H0/H1: the held form is an answerable held class's, past the fence,
            // over a held map.** `held_answerable` is also what makes the held form REQUIRED below.
            let held_answerable = palw_held_class_is_answerable_v1(&builder.state, builder.params, builder.extras, &claim.class_id);
            if held_sub_roots.is_some() {
                if !held_answerable {
                    return Err(PalwStateV2Error::HeldRootClaimRefused {
                        session: *session_id,
                        why: "the held root claim is an answerable held class's: this class is not held, or no honest party can \
                              dissect it inside a turn",
                    });
                }
                if !crate::palw_state_chunk_map::palw_profile_is_held_v4(&binding.shape_profile) {
                    return Err(PalwStateV2Error::HeldRootClaimRefused {
                        session: *session_id,
                        why: "the held root claim's binding does not register the held map",
                    });
                }
            }
            // ADR-0088 Decision 3: the openings prove against the root the claim was admitted
            // against, when the fold recorded one — a line's version, not the class's founding root.
            let proven_root = builder.state.claim_roots.get(&session.claim).copied().unwrap_or(class.artifact_root);
            let operands = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(operand_openings, proven_root)
                .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            // **C-01 (mainnet audit 2026-09-11): the dissection binds a CANONICAL leaf count.**
            // The site is classified under the canonical count `step_leaf_count(profile, context)`
            // (`canonical_step_coordinates`), but the opened output tile and every prefix row are
            // authenticated under the DECLARED `binding.step_leaf_count` (its Merkle tree's shape is
            // a function of that count). The arithmetic path pins the two equal
            // (`StepLeafCountNotCanonical`, `palw_step_leg.rs`); the dissection path did not, so a
            // non-canonical count let the accused commit a step tree the court classifies one way and
            // authenticates another — a terminal remap the honest close cannot convict (it "roots
            // elsewhere"), acquitting a forged execution. Past the audit fence, refuse a
            // non-canonical count HERE: a session can then be opened only over a canonical binding,
            // so the accused's only moves are a canonical one (no remap) or a no-show (which
            // convicts responder-side). The cap is the claim's OWN count, never `PALW_STEP_MAX_LEAVES`
            // — spelling it a constant convicts the honest graph-v5 class whose canonical job counts
            // millions of leaves (the reason `palw_step_leg.rs` reads it this way); `Ok(n) && n ==
            // claim` is the same predicate at every ladder. Below the fence, unchanged. C-01 is one of
            // the DEEP findings, so it rides `palw_audit_2026_09_11_deep` (a later flag day than the
            // shallow fence), not `palw_audit_2026_09_11`.
            if builder.extras.audit_2026_09_11_deep_active {
                match crate::palw_step::step_leaf_count_capped_v1(
                    &binding.shape_profile,
                    &binding.job_context,
                    binding.step_leaf_count,
                ) {
                    Ok(count) if count == binding.step_leaf_count => {}
                    _ => {
                        return Err(PalwStateV2Error::DissectionRefused(
                            *session_id,
                            "the root claim's step_leaf_count is not the canonical function of (profile, context) (C-01)".to_string(),
                        ));
                    }
                }
            }
            // The site is the CLASS's description of the leaf the ladder terminated on — derived
            // from the coordinate and the registered profile, never supplied by the mover.
            // ADR-0119 Decision 4: under the held regime the site's rows open at the claim's ladder.
            let opening_cap = crate::palw_court_v2::palw_attn_opening_cap_v1(
                &builder.state,
                &claim.class_id,
                builder.extras.held_context_ladder.unwrap_or(crate::palw_step::PALW_STEP_MAX_LEAVES),
                builder.extras.held_context_ladder.is_some(),
            );
            let derived =
                crate::palw_court_v2::palw_attn_dispute_site_v3(&claim, binding, &operands, narrowed, filed_anchor, opening_cap)
                    .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            // The anchor the bottom will need: carried and exactly the site's own, or — past the
            // fence, at a site whose bottom reads a checkpoint — required.
            let needs_anchor = derived.site.every_position_is_checkpointed && derived.site.anchor_covered_decode_call.is_some();
            // **ADR-0152 §4-ter: past the fence an answerable held class files the held form or
            // nothing** — the plain and anchored forms are refused where the bottom reads a
            // checkpoint, so the choice left to the responder is to file the sub-roots or to be
            // convicted by its silence (C1).
            if held_answerable && needs_anchor && held_sub_roots.is_none() {
                return Err(PalwStateV2Error::HeldRootClaimRefused {
                    session: *session_id,
                    why: "past palw_offence_attribution an answerable held class's root claim must carry its anchor's slice \
                          sub-roots (CourtAttnRootClaimedHeld, ADR-0152 §4-ter C2)",
                });
            }
            match filed_anchor {
                Some(anchor) => crate::palw_attn_court_v1::palw_attn_anchor_is_the_sites_v1(anchor, &derived.binding, &derived.site)
                    .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?,
                None if builder.extras.attn_anchored_root_active && needs_anchor => {
                    return Err(PalwStateV2Error::DissectionRefused(
                        *session_id,
                        "past palw_attn_anchored_root a root claim whose bottom reads a checkpoint must carry that checkpoint \
                         (CourtAttnRootClaimedAnchored, ADR-0093 Decision 8)"
                            .to_string(),
                    ));
                }
                None => {}
            }
            // **H2/H3: the sub-roots are the anchor's own.** Counted against the held layout the
            // site derives at the anchor, folded through the held map's top tree, compared with the
            // anchor's committed `state_chunks_root` — so every sub-root a bottom's top path is
            // rebuilt from is the accused's.
            if let Some(sub_roots) = held_sub_roots {
                let Some(anchor) = filed_anchor.filter(|_| needs_anchor) else {
                    return Err(PalwStateV2Error::HeldRootClaimRefused {
                        session: *session_id,
                        why: "the held root claim is for a site whose bottom reads a checkpoint",
                    });
                };
                let layout = crate::palw_state_chunk_map::palw_state_layout_v4(&binding.shape_profile, derived.site.anchor_positions)
                    .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
                if sub_roots.len() != layout.slice_count() as usize {
                    return Err(PalwStateV2Error::HeldSubRootCount {
                        session: *session_id,
                        got: sub_roots.len(),
                        want: layout.slice_count() as usize,
                    });
                }
                let rooted = crate::palw_state_chunk_map::palw_state_top_root_from_sub_roots_v4(&layout, sub_roots)
                    .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
                if rooted != anchor.leaf.state_chunks_root {
                    return Err(PalwStateV2Error::HeldSubRootsDoNotRoot {
                        session: *session_id,
                        folded: rooted,
                        committed: anchor.leaf.state_chunks_root,
                    });
                }
            }
            // **The committed row, opened.** This is the pin that makes the whole protocol be
            // about THIS execution: without it a responder whose committed output was forged
            // could state the honest row's partials, play every round truthfully, and be
            // acquitted for an execution that committed something else.
            if out_tile.opening.leaf_index != narrowed {
                return Err(PalwStateV2Error::DissectionRefused(
                    *session_id,
                    "the opened output tile is not the leaf the ladder narrowed to".to_string(),
                ));
            }
            let committed =
                crate::palw_attn_court_v1::palw_attn_opened_lanes_v1(out_tile, &derived.binding, derived.head_lanes.2 as usize)
                    .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            // **ADR-0152 §4-ter (the review's F5, decision): the challenger's first choice is a
            // COMPUTE move** — it re-executes to the site (N2) once the filing is on the chain. The
            // phase opens with the class's compute turn for a held root claim (the profile is the
            // filing's own binding, the class's), and the first disclosure carries that window over
            // to the choice (`CourtAttnDissected` below). Every other phase keeps the court's turn.
            let first_window = if held_sub_roots.is_some() {
                crate::palw_class_admission_v2::palw_held_move_turn_daa_v1(&binding.shape_profile, builder.params.turn_deadline_daa())
            } else {
                builder.params.turn_deadline_daa()
            };
            let phase = crate::palw_attn_court_v1::PalwAttnDissectPhaseV1::open_with_arity(
                *session_id,
                root,
                derived.head_lanes,
                derived.history_positions,
                &committed,
                derived.site.params.values,
                *arity,
                derived.tile_positions,
                ctx.daa_score,
                first_window,
                true,
            )
            .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            session.dissection = Some(phase);
            cap_session_rung_deadline_v2(&mut session, builder.params);
            builder.write_court(*session_id, Some(session))?;
        }
        PalwConsensusObjectV2::CourtAttnDissected { session_id, round, signature: _ } => {
            let mut session =
                builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            let claim_of_session =
                builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();
            let phase = session.dissection.as_mut().ok_or(PalwStateV2Error::NoDissection(*session_id))?;
            // **ADR-0152 §4-ter (the review's F5, decision): the challenger's first choice keeps the
            // compute window move 1 opened.** For an answerable held class past the fence, the first
            // disclosure's deadline was the root claim's DAA plus the class's compute turn; the choice
            // after it is due no earlier than that deadline, and never less than a turn after the
            // disclosure. Every later move is a response and takes the court's turn.
            let base = builder.params.turn_deadline_daa();
            let window = if phase.round() == 0
                && palw_held_class_is_answerable_v1(&builder.state, builder.params, builder.extras, &claim_of_session.class_id)
            {
                base.max(phase.last_deadline_daa().saturating_sub(ctx.daa_score))
            } else {
                base
            };
            phase
                .apply_round(round, ctx.daa_score, window)
                .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            cap_session_rung_deadline_v2(&mut session, builder.params);
            builder.write_court(*session_id, Some(session))?;
        }
        PalwConsensusObjectV2::CourtAttnChildChosen { session_id, choice, signature: _ } => {
            let mut session =
                builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            let phase = session.dissection.as_mut().ok_or(PalwStateV2Error::NoDissection(*session_id))?;
            phase
                .apply_choice(choice, ctx.daa_score, builder.params.turn_deadline_daa())
                .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            cap_session_rung_deadline_v2(&mut session, builder.params);
            builder.write_court(*session_id, Some(session))?;
        }
        // -------------------------------------------------------------------------------------
        // RFC-0002 Phase F, F7 — the IR history dissection's three moves (spec 04b §9.5)
        //
        // ADR-0082's arms over PALW-TIR-32: each refuses a move that is not legal in this session
        // and hands the message to `PalwTirDissectPhaseV1`, whose rules are not restated here. The
        // phase is a row of `tir_dissections` beside the session, written BEFORE `write_court`,
        // which re-indexes the session under the phase's clock. The fence (the k-ary court), the
        // signatures, the arity against the ruleset and the root claim's finalize — a demand
        // evaluation, bounded by the IR court's limits — are the acceptance layer's
        // (`check_court_tir_*_v1`), the split every court move uses. `palw_tir_v1` is the lock above.
        // -------------------------------------------------------------------------------------
        PalwConsensusObjectV2::CourtTirRootClaimed { session_id, root, arity, signature: _ } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            if session.dissection.is_some() || builder.state.tir_dissections.contains_key(session_id) {
                return Err(PalwStateV2Error::DissectionAlreadyOpen(*session_id));
            }
            // A dissection begins where the ladder ends — for a held dissection, at the leaf the
            // accusation named (`open_at_named_leaf`).
            let narrowed = session.ladder.terminal_index().ok_or(PalwStateV2Error::LadderNotTerminal(*session_id))?;
            let claim = builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();
            let refused = |why: String| PalwStateV2Error::DissectionRefused(*session_id, why);
            if !builder.state.tir_classes.contains_key(&claim.class_id) {
                return Err(refused("an IR root claim in a session about a class that is not an IR program".into()));
            }
            // The binding is the claim's — carried with its program empty, the registered class's put
            // back — and the site is the CLASS's description of the narrowed leaf, derived from the
            // program, never supplied by the mover.
            let mut filled = root.as_ref().clone();
            filled.finalize.binding = crate::palw_court_v2::tir_binding_filled_v1(&builder.state, &claim, &root.finalize.binding)
                .map_err(|e| refused(e.to_string()))?;
            crate::palw_court_v2::check_tir_binding_is_the_claims_v1(&builder.state, &claim, &filled.finalize.binding)
                .map_err(|e| refused(e.to_string()))?;
            let site = crate::palw_tir_court_v1::palw_tir_root_claim_site_v1(&filled, narrowed).map_err(refused)?;
            let mut phase = crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1::open(
                *session_id,
                narrowed,
                &site,
                root,
                *arity,
                ctx.daa_score,
                builder.params.turn_deadline_daa(),
            )
            .map_err(|e| refused(e.to_string()))?;
            cap_tir_phase_deadline_v1(&mut phase, &session, builder.params);
            builder.write_tir_dissection(*session_id, Some(phase))?;
            builder.write_court(*session_id, Some(session))?;
        }
        // **RFC-0003: a pipeline claim's root claim opens F7's phase** — the IR arm above over the
        // generative registry: the claim's class must be a registered pipeline, the binding the
        // claim's, and the site the class's description of the narrowed leaf, derived from its
        // program and the job's trips (`palw_gen_root_claim_site_v1`), never supplied by the mover.
        // The finalize is the acceptance layer's; `palw_gen_v1` is the lock above.
        PalwConsensusObjectV2::CourtGenRootClaimed { session_id, root, arity, signature: _ } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            if session.dissection.is_some() || builder.state.tir_dissections.contains_key(session_id) {
                return Err(PalwStateV2Error::DissectionAlreadyOpen(*session_id));
            }
            let narrowed = session.ladder.terminal_index().ok_or(PalwStateV2Error::LadderNotTerminal(*session_id))?;
            let claim = builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();
            let refused = |why: String| PalwStateV2Error::DissectionRefused(*session_id, why);
            let row = builder
                .state
                .gen_classes
                .get(&claim.class_id)
                .ok_or_else(|| refused("a generative root claim in a session about a class that is not a pipeline".into()))?;
            let site =
                crate::palw_gen_close_v1::palw_gen_root_claim_site_v1(root, row, &claim.class_id, &claim.execution_root, narrowed)
                    .map_err(refused)?;
            let mut phase = crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1::open_parts(
                *session_id,
                narrowed,
                &site,
                root.version,
                &root.elements,
                &root.totals,
                *arity,
                ctx.daa_score,
                builder.params.turn_deadline_daa(),
            )
            .map_err(|e| refused(e.to_string()))?;
            cap_tir_phase_deadline_v1(&mut phase, &session, builder.params);
            builder.write_tir_dissection(*session_id, Some(phase))?;
            builder.write_court(*session_id, Some(session))?;
        }
        // **RFC-0004 A6: an evaluation claim's root claim opens F7's phase** — the generative arm above over the
        // claim's own evaluation context: the claim must be the evaluation claim the job table holds for the
        // binding's job, the binding the claim's, and the site the context's description of the narrowed leaf,
        // derived from the class's program and the job's trips (`palw_eval_root_claim_site_v1`), never supplied by
        // the mover. The finalize is the acceptance layer's; `palw_improvement_v1` is the lock above.
        PalwConsensusObjectV2::CourtEvalRootClaimed { session_id, root, arity, signature: _ } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            if session.dissection.is_some() || builder.state.tir_dissections.contains_key(session_id) {
                return Err(PalwStateV2Error::DissectionAlreadyOpen(*session_id));
            }
            let narrowed = session.ladder.terminal_index().ok_or(PalwStateV2Error::LadderNotTerminal(*session_id))?;
            let claim = builder.state.claims.get(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.clone();
            let refused = |why: String| PalwStateV2Error::DissectionRefused(*session_id, why);
            let site = {
                let facts = builder
                    .state
                    .improvement_eval_claim_facts_v1(&claim, &root.finalize.binding)
                    .map_err(|why| refused(why.into()))?;
                crate::palw_improve_eval_court_v1::palw_eval_root_claim_site_v1(root, &facts, narrowed).map_err(refused)?
            };
            let mut phase = crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1::open_parts(
                *session_id,
                narrowed,
                &site,
                root.version,
                &root.elements,
                &root.totals,
                *arity,
                ctx.daa_score,
                builder.params.turn_deadline_daa(),
            )
            .map_err(|e| refused(e.to_string()))?;
            cap_tir_phase_deadline_v1(&mut phase, &session, builder.params);
            builder.write_tir_dissection(*session_id, Some(phase))?;
            builder.write_court(*session_id, Some(session))?;
        }
        PalwConsensusObjectV2::CourtTirDissected { session_id, round, signature: _ } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            let mut phase =
                builder.state.tir_dissections.get(session_id).ok_or(PalwStateV2Error::NoDissection(*session_id))?.as_ref().clone();
            phase
                .apply_round(round, ctx.daa_score, builder.params.turn_deadline_daa())
                .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            cap_tir_phase_deadline_v1(&mut phase, &session, builder.params);
            builder.write_tir_dissection(*session_id, Some(phase))?;
            builder.write_court(*session_id, Some(session))?;
        }
        PalwConsensusObjectV2::CourtTirChildChosen { session_id, choice, signature: _ } => {
            let session = builder.state.court_sessions.get(session_id).ok_or(PalwStateV2Error::MissingSession(*session_id))?.clone();
            let mut phase =
                builder.state.tir_dissections.get(session_id).ok_or(PalwStateV2Error::NoDissection(*session_id))?.as_ref().clone();
            phase
                .apply_choice(choice, ctx.daa_score, builder.params.turn_deadline_daa())
                .map_err(|e| PalwStateV2Error::DissectionRefused(*session_id, e.to_string()))?;
            cap_tir_phase_deadline_v1(&mut phase, &session, builder.params);
            builder.write_tir_dissection(*session_id, Some(phase))?;
            builder.write_court(*session_id, Some(session))?;
        }
        PalwConsensusObjectV2::ProducerDefaulted { claim: claim_id, receipts } => {
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            if claim.phase.is_terminal() {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "ProducerDefaulted" });
            }
            // Symmetric to the licensing arm: here the quorum says the producer withheld, so a
            // seat that signed `Valid` is the contradicted one. Punishing only one direction
            // would make the cheap lie obvious.
            // **ADR-0065 D4: no quorum can license this any more.** The acceptance layer already
            // refuses the object (the tally cannot return `ProducerUnavailable` past the fence), so
            // reaching here would mean something bypassed it — and this arm is the one that calls
            // `void_and_slash`. Refusing rather than trusting the filter is the difference between
            // a rule and a convention.
            if builder.unavailable_abstains {
                return Err(PalwStateV2Error::ProducerDefaultRetired(*claim_id));
            }
            // ADR-0100 Decision 4: a claim drawn per shard defaults through a part, never whole.
            if builder.claim_licenses_by_parts(claim_id) {
                return Err(PalwStateV2Error::LicensedByParts(*claim_id));
            }
            let verdicts = palw_seat_verdicts_of_v2(receipts);
            builder.slash_dissenting_seats(claim_id, &claim, &verdicts, false)?;
            builder.slash_silent_seats(claim_id, &claim, &verdicts)?;
            // Decision 7's default is the producer's fault by construction — the panel answered
            // and it did not — so this void takes the stake, unlike the two timeouts, which void
            // a claim nobody was in a position to blame the producer for.
            builder.void_and_slash(*claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
        }
        // **The second IR fence: an IR step demand** (evidence transport C) opens an R-core+ DA
        // session over the one step leaf or step node it names.
        PalwConsensusObjectV2::DefaultAccusedTirStep { accusation } => {
            return open_da_session_tir_step_v1(builder, ctx, accusation);
        }
        // **Pipeline-claim data availability: a pipeline step demand** (spec 17 §17.14) opens an
        // R-core+ DA session over the one step leaf or step node it names.
        PalwConsensusObjectV2::DefaultAccusedPipelineStep { accusation } => {
            return palw_pipeline_da_fold_v1::open_da_session_pipeline_step_v1(builder, ctx, accusation);
        }
        // Tags 84 and 85 are reserved and uninhabited: no object of either exists.
        PalwConsensusObjectV2::ReservedPipelineDa84(never) | PalwConsensusObjectV2::ReservedPipelineDa85(never) => match *never {},
        // **ADR-0062 SA-1: the accusation that CAN take a bond, because somebody has to prove it.**
        PalwConsensusObjectV2::DefaultAccused { claim: claim_id, missing_event_index, accuser, signature: _ } => {
            // ADR-0152 DA-1 (M3): past `palw_rcore_plus` the event accusation opens a session in the
            // side maps; below it, ADR-0062 SA-1 verbatim.
            if builder.params.rcore_plus_active_at(ctx.daa_score) {
                return open_da_session_rcore_v1(
                    builder,
                    ctx,
                    *claim_id,
                    *accuser,
                    crate::palw_da_rcore_v1::PalwDaUnitV1::event_of_index(*missing_event_index),
                    None,
                );
            }
            open_da_session_v2(builder, ctx, *claim_id, *missing_event_index, accuser, true)?;
        }
        // **ADR-0062 SA-2: the refutation, and it is arithmetic.**
        PalwConsensusObjectV2::MaterialDisclosed { claim: claim_id, event_index, disclosure, signature: _ } => {
            if !builder.da_court {
                return Err(PalwStateV2Error::DaCourtDormant);
            }
            if builder.params.rcore_plus_active_at(ctx.daa_score) {
                return Err(PalwStateV2Error::DaV1AnswerRetired("MaterialDisclosed"));
            }
            let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
            let PalwClaimPhaseV2::DefaultDisputed { accused_daa, missing_event_index, accuser, accuser_exposure, resumed } =
                claim.phase.clone()
            else {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "MaterialDisclosed" });
            };
            // ADR-0103 Decision 4: an event answer does not close a held session — the unit accused
            // is a tile, a chunk or a range, and it is answered in that unit.
            if builder.state.held_da_missing.contains_key(claim_id) {
                return Err(PalwStateV2Error::HeldDaSessionMismatch(*claim_id));
            }
            if *event_index != missing_event_index {
                return Err(PalwStateV2Error::DaDisclosureIndexMismatch { want: missing_event_index, got: *event_index });
            }
            // **The event is the one this claim committed to, in the form its class's scheme
            // names** (SA-2). `trace_root` and `execution_root` are the claim's own pinned fields
            // (ADR-0072 Decision 8), so what the disclosure is compared against is a chain fact
            // and not a restatement — and the check is the same arithmetic the court runs before
            // it reads a close: the binding recomputes, and reaches both roots. The default-ladder
            // cap is enough for every tree this opens (rows and tiles, both far below it).
            let (accused_row, accused_tile) = palw_da_event_index_parts_v1(missing_event_index);
            crate::palw_step_refute::check_trace_event_disclosure_v1(
                claim.trace_root,
                claim.execution_root,
                accused_row,
                accused_tile,
                disclosure,
                crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: *claim_id, why: e.to_string() })?;
            // **F1 (SPEC §4.6): the answer must be about THIS claim's job.** A borrowed root's
            // producer can open the lender's events verbatim; past the fence that binding fails the
            // identity rule and is no answer — the session runs out, and the claim voids for
            // withholding with the accuser uncharged.
            if let Some(why) = builder.da_binding_answers_another_job_v1(ctx.daa_score, claim_id, disclosure.binding()) {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: *claim_id, why });
            }
            // **Only the accuser pays** (SA-4). Not the dissenting seats, not the `Unavailable`
            // ones: past ADR-0065 D4 an `Unavailable` vote is an abstention, and charging it
            // re-creates the transport-loss slashing D4 removed — a third of every remote seat's
            // verdicts on testnet-11 were transport. The accuser made a positive, falsifiable
            // claim and the chain can now see it was false, which is the standard the arithmetic
            // court holds an executor to.
            //
            // Capped like every seat-side charge, and for the same reason: `claim.reserved` is
            // `pwu × slash_value_per_pwu`, both chosen by whoever registered the CLASS, so an
            // uncapped charge would let a registrant make accusing its own class ruinous and buy
            // itself immunity from the DA court.
            //
            // The RESERVATION is released by `write_claim` below, where every other exit from the
            // disputed phase releases it too — see the note there on why this is not a line in
            // this arm.
            builder.slash_seat(accuser, accuser_exposure, builder.params.min_collateral_sompi())?;
            // **The pause is given back** (SA-7). The claim's clock stopped at `accused_daa` and
            // starts again here, so its anchor moves forward by exactly the session it just spent
            // answering — see `resume_claim_after_da_session_v2` for the two-accusation sequence
            // that destroyed an honest producer's claim while it was merely restored.
            let paused = palw_da_paused_daa_v2(&claim.phase, ctx.daa_score);
            debug_assert_eq!(paused, ctx.daa_score.saturating_sub(accused_daa), "the pause is the session's own length");
            let restored = resume_claim_after_da_session_v2(*claim_id, &claim, *resumed, paused)?;
            builder.write_claim(*claim_id, Some(restored.clone()))?;
            // §4-quater V6 on the ADR-0062 court (below `palw_rcore_plus`), before the re-arm reads `H`.
            builder.credit_verify_pause_v6(*claim_id, ctx.daa_score.saturating_sub(paused), ctx.daa_score)?;
            // The session's own clock goes with the session — a claim holds at most one deadline,
            // and leaving the disclose entry behind would sweep a claim that is no longer disputed.
            builder.disarm_deadline(*claim_id);
            rearm_claim_after_da_session(builder, ctx, *claim_id, &restored)?;
        }
        PalwConsensusObjectV2::FreePromptCommitted {
            claim: claim_id,
            class_id,
            bond,
            executor_pubkey,
            work_leaves,
            prompt_token_ids_hash,
            // ADR-0145 §5/§6's two execution facts. Read only past the derived-work fence.
            prompt_tokens,
            prompt_token_ids,
            // Audit H-4: the committed length is a field of the JOB and therefore of the claim id;
            // it is no longer part of the WORK IDENTITY, because a prefix of a run is that run.
            // Past `Params::palw_fp_da_pins` it is read for one other thing — the run's retained
            // chunk shape, which is what bounds an accusation's row range (ADR-0072 Decision 8).
            decode_tokens_executed,
            trace_root,
            output_root,
            execution_root,
            trace_chunk_count,
            trace_retention_daa,
            consumed_prefix_state,
            job_pin,
            public_job,
            eval,
        } => {
            if builder.state.claims.contains_key(claim_id) {
                return Err(PalwStateV2Error::DuplicateClaim(*claim_id));
            }
            let bond_record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
            // **The bond must be the signer's, and only the state can say so.** Everything before
            // this point proves the submitter authored the commitment under a key they chose; it
            // does not prove the key belongs to the bond they named. Without this, a stranger
            // creates claims against any published bond outpoint, the chain derives a panel for
            // them, and the victim's collateral is what burns when no material appears. The
            // attempt lane has had exactly this comparison since P0-2.
            if bond_record.pubkey != *executor_pubkey {
                return Err(PalwStateV2Error::BondKeyMismatch(*bond));
            }
            if let PalwBondStatusV2::Retiring { .. } = bond_record.status {
                return Err(PalwStateV2Error::RetiringBond(*bond));
            }
            // **ADR-0152 U2 (S-SPEC §10a): an FP executor commits only with the producer floor posted.**
            if let Some(shortfall) = palw_bond_producer_floor_shortfall_v1(&builder.state, builder.params, bond, ctx.daa_score) {
                let floor = builder.params.min_collateral_sompi();
                return Err(PalwStateV2Error::ProducerBelowFloor { bond: *bond, collateral: floor.saturating_sub(shortfall), floor });
            }
            // ADR-0160 lane liab (AG-3): nor while its bond is frozen (empty map below the fence).
            if crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&builder.state, bond) {
                return Err(PalwStateV2Error::ProducerFrozen { bond: *bond });
            }
            // **RFC-0004 A6 (MIP-17): an evaluation claim** — the arm's one guarded branch. It is off
            // the reward path (no quanta, pwu, receipt rights or weight), priced by nothing, and its
            // context is the chain's; its `Final` pays the subject's escrowed fee
            // (`palw_improve_eval_fold_v1`). Every other commitment goes on below, as before.
            if let Some(eval) = eval {
                return palw_improve_eval_fold_v1::apply_improvement_eval_commitment_v1(
                    builder,
                    ctx,
                    palw_improve_eval_fold_v1::PalwEvalCommitV1 {
                        claim_id,
                        class_id,
                        bond,
                        work_leaves: *work_leaves,
                        prompt_token_ids,
                        decode_tokens_executed: *decode_tokens_executed,
                        trace_root,
                        output_root,
                        execution_root,
                        job_pin,
                        public_job: public_job.as_deref(),
                        eval,
                    },
                );
            }
            // **N4 (REQ-N4-1, `palw_public_evidence_reward_v1`)**: past the fence a free-prompt claim whose required evidence cannot be
            // public — `PanelDa` prompt ids, FP Job V5 images / source ids — takes no new reward or Final weight, so it is not taken
            // at all (as G14-for-rewards refuses a class off the onboarded path). Evaluation claims (above) earn nothing and are not
            // asked. Below the fence `None`, and a claim accepted before it keeps its old ruleset to its end (REQ-N4-2).
            if let Some(why) = crate::palw_ruling_fences_v1::palw_public_evidence_reward_refusal_v1(
                builder.params,
                ctx.daa_score,
                public_job.as_deref(),
            ) {
                return Err(PalwStateV2Error::ClassNotRewardable {
                    class: *class_id,
                    code: crate::palw_ruling_fences_v1::PALW_N4_NONPUBLIC_EVIDENCE_CODE_V1,
                    why: why.to_string(),
                });
            }
            // **G14 G-03: past `palw_tir_public_conviction_v1` a TIR original takes its ORIGINAL frame or nothing.** A commitment on
            // a registered TIR class without its transaction-paired `TFP1` is refused here, before anything is priced or written —
            // so no frameless TIR claim reaches a reward, weight or credit (it cannot route around the one public conviction door).
            // `Ok` below the fence and for every non-TIR class: byte-identical there.
            palw_tir_public_fold_v1::require_original_frame_v1(builder, *claim_id, *class_id)?;
            // **G-01 option (b) / G-05 (`palw_g14_route_required_v1`)**: past the fence a free-prompt claim whose job needs a G14
            // route that is not armed (selection: the TIR public frame on THIS commitment; constraint / prefix / tokenizer: the staged
            // court with its legacy filer; never for non-public evidence) is not taken — no reward, weight or credit; its carrier pays
            // the ordinary fee. The route is read from the fence state, so the job regains reward once its route is armed. `None`
            // below the fence (every shipped network) and for a claim accepted before it.
            if let Some((code, why)) = crate::palw_g14_gates_v1::palw_g14_route_fp_refusal_v1(
                builder.params,
                ctx.daa_score,
                public_job.as_deref(),
                builder.extras.tir_public_originals.contains_key(claim_id),
            ) {
                return Err(PalwStateV2Error::ClassNotRewardable { class: *class_id, code, why });
            }
            let class = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
            if let PalwClassStatusV2::Frozen { .. } = class.status {
                return Err(PalwStateV2Error::FrozenClass(*class_id));
            }
            // **2026-09-24 DoS audit #11: the class gate an attempt passes, on this lane too.**
            //
            // `apply_attempt` asks `check_class_admits_claim` before it prices anything: a class the
            // registry does not admit (`Registered`, `Prefetching`, `Held`, `Candidate`) takes no
            // new claim, and an admitting one takes a claim only while ADR-0137's panel budget (or,
            // before the work target, the class's inflight cap) has room. This arm asked neither.
            // A commitment on a HELD class — held precisely because its ready seats cannot carry
            // its panel — was accepted, drew a panel from those seats and bound them to duty, and
            // the budget the attempt lane is refused by never saw it (the count below it walked
            // attempts only). The lane was a door around the registry's brake.
            //
            // Past the fence the commitment passes the same gate, in the same place relative to
            // the class checks, and `model_registry_inflight` counts it while it is in flight, so
            // the room an attempt reads includes the panels commitments already hold. The base
            // class is never gated (the gate's own first clause), so the floor's free-prompt lane
            // is exactly what it was. Below the fence nothing here runs and the arm is
            // byte-identical.
            // Asked for one quantum here — the least a commitment carries, so the least room it can
            // need — and for the priced quanta below.
            // **ADR-0152 §4-quater V2, on this lane by name**: an NM class (the 2M row at launch) takes
            // no free-prompt claim, a class whose largest run the chain cannot state takes none
            // unmeasured, and a measured class takes none past D_cap — asked before anything is
            // priced, with this claim's own `work_leaves`, and whether or not the gate below runs.
            builder.check_class_verify_admits_v1(
                class_id,
                ctx.daa_score,
                crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::FreePrompt { work_leaves: *work_leaves },
                true,
            )?;
            if builder.extras.audit_2026_09_23_active {
                builder.check_class_admits_claim(class_id, ctx.daa_score, PalwGatedClaimV1::FreePrompt { quanta: 1 })?;
            }
            // **RFC-0002 Part II Proposal A (`palw_class_seating`), on this lane too**: the one seating function every door asks.
            // Lane MU (ADR-0173): a free-prompt claim names no root and is attributed to the founding line's current one
            // (`note_claim_usage` below), so past `palw_audit_1004_v1` the door asks the root the claim will be attributed to.
            let fp_root = builder
                .state
                .model_line_or_founding(class_id)
                .and_then(|line| builder.state.model_version(class_id, line.current).map(|version| version.root));
            builder.read().check_class_seated_root_v1(class_id, fp_root.as_ref(), bond, ctx.daa_score)?;
            if let Some(root) = fp_root.as_ref() {
                builder.read().check_root_possession_v1(class_id, root, Some(bond), ctx.daa_score)?;
            }
            // **ADR-0152 v3.1 T-2(a), on this lane too** (S-6): the executor's share of the class's
            // unlicensed claims, one per commitment whatever its quanta — a commitment draws a
            // panel of its own, as an attempt does. Past `Params::palw_rcore_plus` only; a
            // refusal here is dropped by the acceptance rehearsal, which folds this same arm.
            builder.check_bond_class_share(bond, class_id, ctx.daa_score)?;
            // **Priced in leaves against the class's own canonical job** (ADR-0074 Decision 5):
            // the quantum is a fraction of the class's job size, quanta are whole quanta of the
            // capture's leaves, and pwu is quanta × quantum — the attempt lane's unit.
            let (per_job, cap) = (builder.params.fp_quanta_per_canonical_job, builder.params.fp_max_quanta_per_receipt);
            if per_job == 0 || cap == 0 {
                return Err(PalwStateV2Error::FreePromptLaneUnpriced);
            }
            // **Weight is the price of adjudicability, on this lane too** (ADR-0073 Decision 2,
            // ADR-0074 Decision 6): a class whose free-prompt path no drill has certified takes
            // no free-prompt claim, so nothing it could not be convicted over ever licenses a
            // block.
            if let Some(certified) = &builder.params.fp_certified_classes
                && !certified.contains(class_id)
                && !builder.state.fp_certified_classes.contains_key(class_id)
                && !tir_fp_lane_covered_v1(builder, class_id)
            {
                return Err(PalwStateV2Error::FreePromptLaneUncertified(*class_id));
            }
            // **ADR-0082 Decision 10: what this claim is CREDITED for.**
            //
            // Under `Params::palw_fp_decode_rules` the numerator is the job's DECODE leaves and
            // the prefill of a user-chosen prompt is priced at zero — a deterministic causal model
            // makes every prefix leaf recomputable at zero cost by the bond that computed it once,
            // so a subsidy on prefill is a subsidy on replay. The quantum (the denominator) is
            // unchanged: still a fraction of the class's own canonical job (ADR-0074 Decision 5).
            //
            // `work_leaves` stays on the record whatever the fence says — the seat's binding is
            // against the capture's own count, and the attempt lane reads it — but past the fence
            // it is not what the claim is PAID for.
            //
            // **The decode half is enumerated where the work is DERIVED** (RFC-0001 §A.3 D10):
            // past the fence the price reads the class's published graph and the run's two
            // committed counts (ADR-0145 §5 — `validate_palw_v2` arms the decode rules only over
            // `palw_fp_derived_work` at or below them), and credits the run as if its whole
            // prompt were already paid for — the prefill is priced at zero and the decode calls
            // that ran are what earn; an answer a stop sequence ended early earns for the tokens it
            // decoded and no more. `palw_fp_commitment_price_from_state_v1` below is that one
            // spelling; `FreePromptDecodeLeavesUnavailable` is its answer for a ruleset that armed
            // the rules without the derivation, which no assembled ruleset is.
            // **ADR-0145 §5 and §6: the work is DERIVED, and a prefix already paid for is not paid
            // again.** The 2026-09-19 reward audit's F2.
            //
            // Below the fence this is exactly the line it replaced — `credited = work_leaves` —
            // and `derived` is `None`, so every arithmetic after it is byte for byte what it was.
            // That is the whole of the dormant behaviour, and
            // `a_dormant_derived_work_fence_moves_no_state_root` asserts it against a real fold.
            //
            // Past the fence three things happen in one place, because they are one accounting:
            //
            // 1. the chain recomputes the run's leaves from the class's GRAPH and the run's two
            //    committed token counts, and refuses a commitment whose `work_leaves` disagrees.
            //    Refused, not corrected: a silent correction is a wire format nobody reads, and a
            //    producer whose number is wrong has either a different graph or a different run,
            //    neither of which the chain should paper over (ADR-0145 I1);
            // 2. the chain reads which prefix of this prompt this bond has already been paid for
            //    on this class, from its own record, and credits only the NEW tokens (§6). Nothing
            //    the executor writes is an input to that reading;
            // 3. the claim's prompt is recorded so the next one can be read the same way.
            //
            // The refusals are BY NAME and every one of them is a thing a producer can check
            // before it spends an inference: the class has published a graph, the run's leaves are
            // the graph's answer, and the prompt is not one the chain already bought.
            // ADR-0148 / the entrance's agreement with the ledger: the price is ONE function of the
            // state, which a node also answers a gateway with (`palw_fp_commitment_price_v1`), so the
            // exposure a gateway checks before an inference and the reservation written here are
            // the same number by construction rather than by two readings agreeing.
            let price = palw_fp_commitment_price_from_state_v1(
                &builder.state,
                builder.params,
                PalwFpPriceInputsV1 {
                    fp_derived_work_daa: builder.extras.fp_derived_work_daa,
                    canonical_work_daa: builder.extras.canonical_work_daa,
                    daa_score: ctx.daa_score,
                    receipt_rights: builder.fp_receipt_rights_inputs_v1(ctx),
                    // RFC-0002 Phase H: the IR class lane is open exactly where the TIR public frame is (G-03 frames every IR original).
                    tir_free_prompt_active: builder.extras.tir_public_conviction_active,
                },
                claim_id,
                class_id,
                prompt_token_ids,
                *prompt_tokens,
                *decode_tokens_executed,
                *work_leaves,
                *consumed_prefix_state,
            )?;
            let (quanta, pwu, reserved, in_compute) = (price.quanta, price.pwu, price.reserved, price.priced_in_compute);
            // **2026-09-24 DoS audit review of #11: the gate holds the jobs this claim will count as.**
            // The early gate above asks for one quantum's room; the commitment's quanta are pooled
            // with its class's claims in flight (`palw_inflight_claims_counted_v1`, whole jobs over
            // the class), so it must find the room its priced quanta take there, or it would
            // overshoot the budget it is measured against. Only past the fence, like the gate. (The
            // audit #4 review's item 4, and its verification: pooled before it is counted, a
            // commitment that fills its class's last part-job needs no job of room.)
            if builder.extras.audit_2026_09_23_active && quanta > 1 {
                builder.check_class_admits_claim(class_id, ctx.daa_score, PalwGatedClaimV1::FreePrompt { quanta: quanta as u64 })?;
            }
            let derived = price.derived_work;
            // **G14 detection policy (`palw_detection_policy_v1`, keyed on the acceptance DAA)**: past the fence a commitment whose class
            // has no detection policy, whose N is not bound to its priced work (the derived-work rule re-prices `work_leaves` from the
            // class's graph and the two committed counts — without it the counts are the producer's word and the claim is refused),
            // whose collectable collateral is below the deterrence bound, or that the policy cannot check within its resources, is not
            // taken — option (b)'s refusal: no reward, weight or credit; its carrier pays the ordinary fee. Before any write. `None`
            // below the fence.
            if builder.params.detection_policy_v1().is_some() {
                let collectable = builder.state.bonds.get(bond).map_or(0, |b| u128::from(b.collateral)).min(reserved);
                let facts = crate::palw_detection_policy_v1::PalwDetectionClaimFactsV1 {
                    attempt_lane: false,
                    work_leaves: *work_leaves,
                    context_priced_leaves: derived.is_some().then_some(*work_leaves),
                    prompt_tokens: *prompt_tokens,
                    decode_tokens_executed: *decode_tokens_executed,
                    collateral_sompi: collectable,
                };
                if let Some((code, why)) =
                    crate::palw_detection_policy_v1::palw_detection_claim_refusal_with_budget_v1(
                        builder.params,
                        class_id,
                        ctx.daa_score,
                        &facts,
                        crate::palw_court_capacity_v1::palw_court_claim_budget_for_retention_v1(
                            &builder.state,
                            builder.params,
                            class_id,
                            ctx.daa_score,
                            *trace_retention_daa,
                        ),
                    )
                {
                    return Err(PalwStateV2Error::ClassNotRewardable { class: *class_id, code, why });
                }
            }
            // **Fail-closed on the one field the court cannot do without (audit C3).** A claim
            // carrying no execution root has nothing a refutation's binding can be tested
            // against; the shipped worker commits the binding's own root, so what this refuses
            // is a commitment built by something else.
            if *execution_root == Hash64::default() {
                return Err(PalwStateV2Error::UnadjudicableCommitment(*claim_id));
            }
            // **One inference, one claim** (ADR-0074 Decision 4) — and since audit H-4 the identity
            // is the PROMPT's, not the prompt's plus a length the executor picks: one run
            // committed at every nested decode limit was D truthful claims for one inference.
            let work_id = crate::palw_freeprompt_v3::fp_work_id_v1(class_id, prompt_token_ids_hash, &bond.0);
            if let Some(holder) = builder.state.work_ids.get(&work_id) {
                return Err(PalwStateV2Error::DuplicateWork { work_id, claim: *holder });
            }
            // **Admission item 8, on this lane** (closes the free-prompt half of P0-10): what this
            // bond already backs — live claims and its own registrations — plus what this claim
            // would reserve, against collateral × ratio. A commitment meets no admission list (it
            // arrives in a transaction), so the ceiling is the transition's; the expression is the
            // attempt lane's, and `reserve_for_claim` below adds exactly `reserved`.
            // ADR-0071 SA-2's third term, on this lane and for the same reason: the ceiling
            // applies to the SUM, and a reservation nothing reads is not a reservation. Zero while
            // the fence is off, so this lane's admission is byte-identical before it.
            let declared = if builder.capability_bound { palw_bond_capability_exposure_v1(bond_record) } else { 0 };
            // ADR-0152 SR-7: past `palw_rcore_plus` the one committed ledger, with capability
            // exposure kept as this lane's own gate term (inside `committed` it would pin exit).
            let own = if builder.params.rcore_plus_active_at(ctx.daa_score) {
                builder.committed_at(bond, ctx.daa_score)
            } else {
                builder
                    .state
                    .reserved_exposure(bond)
                    .checked_add(builder.state.registration_exposure(bond))
                    .ok_or(PalwStateV2Error::Overflow("total exposure"))?
            };
            let backed = own.checked_add(declared).ok_or(PalwStateV2Error::Overflow("total exposure"))?;
            // Past the fence the one invariant's room (review M1) holds the declared term too.
            let ceiling = if builder.params.rcore_plus_active_at(ctx.daa_score) {
                own.saturating_add(builder.gate_room(bond, ctx.daa_score, PalwRcoreGateV1::Work))
            } else {
                (bond_record.collateral as u128)
                    .checked_mul(builder.params.fp_max_exposure_ratio_permille as u128)
                    .ok_or(PalwStateV2Error::Overflow("exposure ceiling"))?
                    / 1000
            };
            // **The audit's #5 (escrow half): the receipt rights this claim's Final could realize**,
            // priced now and held with the weight — see `PalwClaimStateV2::rights_reserved`. Only a
            // compute-priced claim past the audit fence; zero otherwise, which keeps every other
            // network's reservation and state byte-identical.
            let rights_reserved = price.rights_reserved;
            let claim_holds = reserved.checked_add(rights_reserved).ok_or(PalwStateV2Error::Overflow("claim reservation"))?;
            let would_reserve = backed.checked_add(claim_holds).ok_or(PalwStateV2Error::Overflow("reserved exposure"))?;
            if would_reserve > ceiling {
                return Err(PalwStateV2Error::FreePromptExposureCeiling { bond: *bond, backed, claim: claim_holds, ceiling });
            }
            let claim = PalwClaimStateV2 {
                source: PalwClaimSourceV2::FreePrompt { quanta, spent: BTreeSet::new() },
                class_id: *class_id,
                bond: *bond,
                pwu,
                accepted_daa: ctx.daa_score,
                rebound_daa: None,
                accepted_blue_score: ctx.blue_score,
                accepted_block: ctx.block,
                trace_root: *trace_root,
                output_root: *output_root,
                execution_root: *execution_root,
                // **The DA obligation is the CHAIN's, not the producer's** (ADR-0072 Decision 8,
                // free-prompt half; mainnet audit 2026-09-06 M-3).
                //
                // Both were carried verbatim, and `trace_retention_daa` is the only bound on when
                // a `DefaultAccused` may be filed (`DaOutsideRetention`, this file's accusation
                // arm). A producer writing `0` was permanently immune to the DA court for free —
                // and so was an honest one, who is *also* better off writing `0` because it
                // immunises against griefing accusations that pause the claim. A field with that
                // shape does not need an attacker; it drifts to `0` on its own, and the court the
                // card arms becomes decorative.
                //
                // The attempt lane pins these by EQUALITY at admission, because an attempt rides
                // its block's header and knows the DAA. A commitment rides an ordinary transaction
                // built before anyone knows which block accepts it, so equality is unsatisfiable
                // and the chain DERIVES instead — the same decision at the only layer holding the
                // input. `ctx.daa_score` is the same value this record's `accepted_daa` takes,
                // which is what the accusation arm anchors on.
                //
                // The chunk count is the run's own shape: `⌈decode_tokens_executed / 256⌉`, the
                // formula `PalwFpWorkerResultV3::validate_against_request` already enforces on the
                // worker. It bounds `palw_da_max_accusable_rows_v1` and nothing else.
                trace_chunk_count: if builder.extras.fp_da_pins_active {
                    decode_tokens_executed.div_ceil(crate::palw_freeprompt_v3::PALW_FP_TRACE_CHUNK_EVENTS_V3).max(1)
                } else {
                    *trace_chunk_count
                },
                trace_retention_daa: if builder.extras.fp_da_pins_active {
                    ctx.daa_score.saturating_add(crate::palw_producer_v2::palw_min_trace_retention_daa_v1(builder.params))
                } else {
                    *trace_retention_daa
                },
                reserved,
                // Zero, deliberately (ADR-0044): a commitment riding a transaction is not a
                // block's work — β credit here would let commitment-stuffing pump a chain's live
                // weight without producing anything.
                immature_contribution: 0,
                // Zero for the SAME reason, and it is what keeps the subsidy bound structural: a
                // block may carry many commitments but only one attempt, so letting a commitment
                // draw the producer carve would let N of them mint N carves out of one subsidy —
                // precisely the "addition to the schedule" ADR-0042 Decision 10 forbids. The
                // receipt lane is paid by the spends it serves, not by the block that carried it.
                escrowed_reward: 0,
                work_leaves: *work_leaves,
                work_id: Some(work_id),
                phase: PalwClaimPhaseV2::Provisional,
                rights_reserved,
                // **ADR-0152 v3.1 J-1 (F1): the commitment's job pin**, past
                // `palw_offence_attribution` only — the identity a binding's context must reproduce
                // (a free-prompt claim's id can never be its job id). R-core+'s fields are dormant.
                job_identity: if builder.extras.offence_attribution_active { *job_pin } else { Hash64::default() },
                rcore: PalwClaimRcoreV1::default(),
            };
            let mut claim = claim;
            let public_job_wire = builder.public_fp_job_wire_v1(claim_id, &claim, public_job.as_deref())?;
            let tir_original = palw_tir_public_fold_v1::prepare_paired_original_v1(builder, ctx, object, public_job.as_deref())?;
            let staged_contract = if let Some(frame) = &tir_original {
                // This is an ORIGINAL prospective allocation, not a repricing
                // of an old claim or a second old required-family bundle.
                claim.trace_retention_daa = claim.trace_retention_daa.max(frame.plan_v1().terms.material_until_daa);
                None
            } else {
                builder.prepare_staged_claim_with_job_v1(*claim_id, &mut claim, public_job.as_deref())?
            };
            // **Lane BUDGET (ADR-0176 D3): the commitment's reservation** — Q 1, B `quanta` receipt blocks, R `quanta` carves of this
            // block, F `quanta` quanta of weight — before the first write; refused by name. `None` below `palw_bond_budget_v1`.
            builder.state.require_native_monetary_material_scope_v1(builder.params, *claim_id, &claim)?;
            let consumer_plan = builder.plan_paid_consumer_v1(*claim_id, &claim)?;
            // RFC-0017 (REQ-ELIG-2): a `Probation` model's commitment is admitted whole inside its stage cap and the network's
            // probation budget (`quanta` receipt blocks), planned with F = 0 and pinned once written; `None` below the fence.
            let probation_ticket = super::palw_model_eligibility_fold_v1::eligibility_probation_preview_v1(
                builder,
                ctx,
                &claim.class_id,
                &claim.bond,
                crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1.saturating_mul(quanta as u64),
            )?;
            let budget_plan = builder.plan_free_prompt_budget_v1(ctx, &claim, quanta)?;
            let budget_plan = match (&probation_ticket, budget_plan) {
                (Some(ticket), Some(plan)) if ticket.weightless() => Some(plan.without_final_weight_v1()),
                (_, plan) => plan,
            };
            let panel_authorization = palw_panel_lifetime_fold_v2::select_authorization(builder, &claim)?;
            builder.reserve_for_claim(&claim)?;
            if let Some(contract) = consumer_plan {
                builder.commit_paid_consumer_v1(*claim_id, contract)?;
            }
            let fp_pwu = claim.pwu;
            builder.write_claim(*claim_id, Some(claim))?;
            builder.stamp_staged_claim_v1(*claim_id, staged_contract)?;
            if let Some(wire) = public_job_wire {
                builder.write_public_fp_job_v1(*claim_id, Some(wire))?;
            }
            if let Some(plan) = budget_plan {
                builder.commit_bond_budget_v1(ctx, *claim_id, plan)?;
            }
            if let Some(ticket) = probation_ticket {
                super::palw_model_eligibility_fold_v1::eligibility_probation_commit_v1(builder, ctx, claim_id, ticket)?;
            }
            // **ADR-0145 §6: what this claim bought, so the next one is not sold it twice.**
            //
            // Written after `write_claim` because `write_claim` is also where the row is DROPPED,
            // and a row written before the claim exists would be swept by the drop of a claim that
            // was never there. Only past the fence: below it the map stays empty, which is what
            // keeps the state root where it was.
            let prompt_row = PalwFpClaimPromptV1 {
                prompt_tokens: *prompt_tokens,
                prefix_root: crate::palw_freeprompt_v3::fp_prompt_prefix_root_v1(prompt_token_ids),
                prompt_token_ids: prompt_token_ids.clone(),
                expires_daa: palw_fp_prompt_row_expiry_v1(ctx.daa_score, builder.params.epoch_length),
                consumed_state_root: consumed_prefix_state.state_root,
                consumed_prefix_tokens: consumed_prefix_state.prefix_tokens,
            };
            if derived.is_some() {
                // ADR-0148: the first compute-priced commitment opens the lane's one target.
                if in_compute {
                    builder.seed_fp_pooled_receipt_target()?;
                }
                builder.write_fp_claim_prompt((*class_id, *bond, *claim_id), Some(prompt_row.clone()))?;
            }
            // ADR-0088 Decision 3's recorded gap: a free-prompt claim names no root, so it is
            // attributed to the class's founding line's current root until the job's v6.
            if let Some(line) = builder.state.model_line_or_founding(class_id)
                && let Some(current) = builder.state.model_version(class_id, line.current)
            {
                builder.note_claim_usage(*claim_id, class_id, current.root, fp_pwu, false, ctx.daa_score)?;
            }
            let deadline = ctx.daa_score.checked_add(builder.params.window_bind).ok_or(PalwStateV2Error::Overflow("bind deadline"))?;
            // RFC-0010: a claim accepted under the permissionless Panel's rule is clocked by its engine, not by V2's bind window.
            if !builder.params.panel_v3_rule_at(ctx.daa_score) {
                builder.arm_deadline(deadline, *claim_id)?;
            }
            // No production census here: commitments are not blocks. The receipt lane's counters
            // move when a quantum is SPENT.
            if let Some(authorization) = panel_authorization {
                palw_panel_lifetime_fold_v2::capture(builder, *claim_id, authorization)
                    .expect("a prevalidated native claim must have its immutable Panel consent fields");
            }
            if let Some(frame) = tir_original {
                // G14 G-T3: the frame keeps the claim's public prompt ids to its material horizon (derived work or not).
                palw_tir_public_fold_v1::install_original_v1(builder, ctx, frame, &prompt_row)?;
            }
        }
    }
    Ok(())
}

/// **RFC-0002 Phase H: an IR class's free-prompt lane is authenticated by coverage** — past the TIR public frame's fence (the IR
/// lane's), a registered IR class whose every reachable primitive one certified family covers (either lane: the generic interpreter
/// court is the same court for an attempt and a free prompt; the free-prompt doors are the frame's, in consensus code). `false`
/// below the fence and for every non-IR class, so the legacy gate is byte-identical there. No per-class drill, no model review.
fn tir_fp_lane_covered_v1(builder: &TransitionBuilder<'_>, class_id: &Hash64) -> bool {
    if !builder.extras.tir_public_conviction_active {
        return false;
    }
    let Some(record) = builder.state.tir_class_v1(class_id) else { return false };
    let families = builder
        .state
        .certified_family_records(PalwCertifiedLaneV1::Attempt)
        .values()
        .chain(builder.state.certified_family_records(PalwCertifiedLaneV1::FreePrompt).values())
        .map(|record| &record.family);
    crate::palw_tir_fp_lane_v1::palw_tir_fp_lane_covered_v1(record, families).is_ok()
}

/// **D2 on seat readiness (REQ-T6-3, restricted; `palw_court_model_scope_v1`)**: `SeatReadinessLeaves` publishes ~16 plaintext
/// artifact leaves per (class, bond, span) as a seat condition. Past the fence a class REGISTERED at/after it — all of whose claims are
/// new — takes no readiness proof (a Panel that cannot seat is a non-fraud, uncharged non-acceptance: REQ-T6-2); a class registered
/// before keeps its readiness for the old claims that seat through it. `Ok` below the fence (every shipped chain).
fn check_readiness_model_scope_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    class_id: &Hash64,
) -> Result<(), PalwStateV2Error> {
    let registered = builder.state.class(class_id).map(|class| class.registered_daa);
    crate::palw_ruling_fences_v1::palw_court_model_scope_admits_readiness_v1(builder.params, ctx.daa_score, registered)
        .map_err(|why| PalwStateV2Error::ReadinessProofRefused(why.to_string()))
}
