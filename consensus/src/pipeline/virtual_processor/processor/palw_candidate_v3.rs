//! Rebuildable PALW preparation for one actual UTXO acceptance candidate.
//! The chain owner alone disqualifies or commits a block. This result owns
//! its next state, delta leases, retained materials and deferred court events;
//! dropping it publishes none of those outputs. Selection and canonical fold
//! still execute the same checks and in the same order as the original walk.

use super::*;

pub(super) struct PreparedPalwCandidateV3 {
    pub next: kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1,
    pub staged: PalwUtxoCommitV2,
    pub observation: Option<kaspa_consensus_core::palw_court_observation_v1::CommittedCourtObservationV1>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeManifestCandidateModeV3 {
    ProjectFees,
    ProjectTemplateFees,
    Strict,
}

/// Immutable inputs that a signed, unsubmitted template cannot read from
/// stores under its new hash. Only construction from a matching virtual
/// generation is exposed; the final block's acceptance still resolves its
/// own GHOSTDAG/window/payload independently.
#[derive(Clone, Copy)]
pub(super) struct NativeManifestTemplateInputsV3<'a> {
    parent: BlockHash,
    pub(super) non_daa: &'a kaspa_consensus_core::BlockHashSet,
    pub(super) evm_payload: &'a kaspa_consensus_core::evm::EvmExecutionPayload,
}
impl<'a> NativeManifestTemplateInputsV3<'a> {
    pub(super) fn from_virtual_v3(
        virtual_state: &'a VirtualState,
        header: &Header,
        evm_payload: &'a kaspa_consensus_core::evm::EvmExecutionPayload,
    ) -> Result<Self, PalwLocalTransitionFaultV1> {
        if header.hash == BlockHash::default()
            || header.hash == kaspa_consensus_core::blockhash::ORIGIN
            || header.direct_parents() != virtual_state.parents.as_slice()
            || header.daa_score != virtual_state.daa_score
            || header.blue_score != virtual_state.ghostdag_data.blue_score
            || header.blue_work != virtual_state.ghostdag_data.blue_work
            || header.bits != virtual_state.bits
        {
            return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                "Manifest template projection",
                "signed header differs from its captured virtual generation",
            )
            .into());
        }
        Ok(Self { parent: virtual_state.ghostdag_data.selected_parent, non_daa: &virtual_state.mergeset_non_daa, evm_payload })
    }
}

/// Supplied by the candidate driver, never a preset or wire input. The same
/// non-Clone replay budget survives selection changes; pricing includes native
/// archive preparation rather than an isolated request-payload tariff.
pub(super) struct NativeManifestCandidateInputV3<'a, 'parent> {
    pub mode: NativeManifestCandidateModeV3,
    pub template: Option<NativeManifestTemplateInputsV3<'a>>,
    /// Present only for a disposable fee projection; cannot supply a strict
    /// fold or an EVM stage. Strict candidates use validated ctx.evm_staged.
    pub projected_evm_actions: Option<Vec<kaspa_consensus_core::evm::model_market::PalwEvmMarketActionV1>>,
    pub maximum_archive_heap: u64,
    pub maximum_archive_work: u64,
    pub replay_budget: &'a mut kaspa_consensus_core::palw_state_v2::PalwOrderedReplayBudgetV1<'parent>,
    pub quote: &'a mut dyn FnMut(
        kaspa_consensus_core::palw_court_service_v1::ServiceCost,
        u64,
        u64,
    ) -> Result<u64, misaka_palw_positive_contract::ContractError>,
}

pub(super) enum PalwCandidateResultV3 {
    Projected {
        fees: kaspa_consensus_core::palw_state_v2::PalwManifestRequestFeeProjectionV3,
        relay: palw_accept::NativeRelayFeePlanV2,
        fees_match_actual: bool,
    },
    /// Stable disposable acceptance/rewards only. It carries no canonical
    /// PALW next state, verified EVM stage or authority to commit a block.
    TemplateReady,
    Prepared(PreparedPalwCandidateV3),
    Refused {
        stage: &'static str,
        why: String,
    },
}

impl VirtualStateProcessor {
    /// The chain and signed-template trial must start with identical locks,
    /// obligations and round authority from the same immutable parent.
    pub(super) fn populate_palw_candidate_context_v3(
        &self,
        ctx: &mut UtxoProcessingContext<'_>,
        state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
        header: &Header,
        non_daa: &BlockHashSet,
    ) -> Result<(), PalwLocalTransitionFaultV1> {
        let daa = header.daa_score;
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: header.hash,
            daa_score: daa,
            blue_score: header.blue_score,
            subsidy: self.coinbase_manager.calc_block_subsidy(daa),
        };
        let fault = |why: &str| {
            kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>("candidate funding locks", why)
        };
        ctx.palw_v2_payout_outputs = state.map(|s| self.palw_v2_payout_outputs(s)).transpose()?.unwrap_or_default();
        ctx.palw_v2_locked_bonds = state.map(|s| self.palw_v2_locked_bond_outpoints(s, daa)).transpose()?.unwrap_or_default();
        if state.is_some() {
            ctx.palw_v2_locked_bonds.extend(self.palw_v2_bonds_declared_in_mergeset(ctx));
        }
        ctx.palw_v2_escrow_withheld = state.map(|s| self.palw_v2_escrow_withheld_at(s, ctx.selected_parent(), daa)).unwrap_or(0);
        ctx.palw_v2_unentitled_blues = state
            .map(|s| self.palw_v2_unentitled_blues_checked_v1(s, &ctx.ghostdag_data, non_daa, &point))
            .transpose()?
            .unwrap_or_default();
        ctx.palw_v2_receipt_v4_payouts = state
            .map(|s| self.palw_v2_receipt_v4_payouts(s, &ctx.ghostdag_data, non_daa, &ctx.palw_v2_unentitled_blues))
            .unwrap_or_default();
        ctx.palw_v2_merged_escrow_withheld = state
            .map(|s| self.palw_v2_merged_escrow_withheld_checked_v1(s, &ctx.ghostdag_data, non_daa, &point))
            .transpose()?
            .unwrap_or_default();
        ctx.palw_v2_bond_burns = state.map(|s| self.palw_v2_bond_burn_obligations(s, daa)).transpose()?.unwrap_or_default();
        ctx.palw_escrow_locks.rows = state.map(|s| self.palw_escrow_locked_rows_v1(s, daa)).unwrap_or_default();
        ctx.palw_escrow_locks.protected_outpoints = state.map(|s| self.palw_protected_escrow_outpoints_v1(s)).unwrap_or_default();
        ctx.palw_escrow_locks.assignment = state
            .map(kaspa_consensus_core::palw_positive_assignment_funds_v2::assignment_funding_locks_v2)
            .transpose()
            .map_err(|why| fault(&format!("{why:?}")))?
            .unwrap_or_default();
        ctx.palw_escrow_locks.positive_case = state
            .map(kaspa_consensus_core::palw_positive_assignment_funds_v2::positive_case_funding_locks_v2)
            .transpose()
            .map_err(|why| fault(&format!("{why:?}")))?
            .unwrap_or_default();
        ctx.palw_escrow_locks.assignment_prover = state
            .map(kaspa_consensus_core::palw_positive_assignment_funds_v2::assignment_prover_funding_locks_v2)
            .transpose()
            .map_err(|why| fault(&format!("{why:?}")))?
            .unwrap_or_default();
        ctx.palw_round_verdicts =
            state.map(|s| self.palw_round_verdicts_at_v1(s, &ctx.ghostdag_data, daa, header.hash)).transpose()?.flatten();
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_palw_candidate_v3<V: UtxoView + Sync>(
        &self,
        current: BlockHash,
        header: &Arc<Header>,
        ctx: &mut UtxoProcessingContext<'_>,
        selected_parent_utxo_view: &V,
        selected_parent_bond_view: &ActiveBondView,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        assignment_raw_actions: Vec<kaspa_consensus_core::palw_state_v2::PositiveAssignmentRawActionV2>,
        tokenizer_prefix: Option<&tokenizer_registration_v1::TokenizerRegistrationFeePrefixV1<'_>>,
        exec_resolved: Option<&PalwExecV2ResolvedV1>,
        mut manifest: Option<NativeManifestCandidateInputV3<'_, '_>>,
    ) -> Result<PalwCandidateResultV3, PalwLocalTransitionFaultV1> {
        if manifest.is_some() != ctx.palw_manifest_requests_v3.is_some() {
            return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                "PALW candidate Manifest owner",
                "actual UTXO pass and complete pricing owner must agree",
            )
            .into());
        }
        let projecting = manifest.as_ref().is_some_and(|input| input.mode != NativeManifestCandidateModeV3::Strict);
        if manifest.as_ref().is_some_and(|input| {
            (input.mode == NativeManifestCandidateModeV3::ProjectTemplateFees) != input.template.is_some()
                || input.template.is_some_and(|template| template.parent != ctx.selected_parent() || header.hash != current)
        }) {
            return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                "Manifest candidate mode",
                "template inputs and candidate context differ",
            )
            .into());
        }
        if !projecting && manifest.as_ref().is_some_and(|input| input.projected_evm_actions.is_some()) {
            return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                "PALW strict candidate",
                "projected EVM actions cannot supply a strict fold",
            )
            .into());
        }
        if projecting {
            ctx.require_no_evm_stage_v3()?;
        }
        let pov_daa_score = header.daa_score;
        // One block's paid whole-root work survives the acceptance walk,
        // nested rehearsals and final fold, despite other threads' work.
        let _prompt_roots = kaspa_consensus_core::palw_attempt_rules_v1::PalwPromptRootBlockGuardV2::enter(current);
        let state_params = self.palw_state_params_v2.as_ref().expect("palw_state is Some only when the params are");
        // Unit C step 5: the header commits to the state this block's
        // transition STARTS from — the parent's root — so it is checked
        // BEFORE the transition runs, against the state this node's own
        // walk reached. Without it the root is something each node
        // computes privately and nobody can be held to.
        //
        // A mismatch disqualifies the block rather than the node: the
        // producer built on a state this chain does not have, and that is
        // a fact about the block.
        let parent_root = self.palw_committed_root_v1(state);
        if header.palw_state_root != parent_root {
            return Ok(PalwCandidateResultV3::Refused {
                stage: "state root",
                why: format!("committed {}, this chain is at {}", header.palw_state_root, parent_root),
            });
        }
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: current,
            daa_score: header.daa_score,
            blue_score: header.blue_score,
            // The pool this block's claims escrow from (ADR-0042
            // Decision 10), taken from the ONE emission schedule rather
            // than a second copy of it.
            subsidy: self.coinbase_manager.calc_block_subsidy(header.daa_score),
        };
        // Unit C step 3: the block's own objects, in its own acceptance
        // order. Read from the acceptance data this validation just
        // produced rather than from the store — the store row is written
        // by the commit below, so reading it here would be reading a fact
        // that does not exist yet.
        let (objects, batch_candidates, tokenizer_candidates) = self.palw_v2_objects_of_block(
            &ctx.mergeset_acceptance_data,
            state,
            current,
            header.daa_score,
            // ADR-0075 SA-1/SA-2: what each 0x4b carrier paid, collected
            // by the UTXO walk two frames up, where the fee is knowable.
            &ctx.palw_v2_accepted_tx_fees,
            &ctx.palw_v2_rent_available_fees,
            &ctx.palw_batch_handoffs_v1,
            &ctx.palw_tokenizer_handoffs_v1,
            &ctx.palw_tfp1_history_seeds_v1,
        )?;
        let history_custody_pledges =
            objects.iter().filter_map(|carried| carried.history_pledge.clone()).map(|pledge| (pledge.claim, pledge)).collect();
        let public_original_bindings = objects
            .iter()
            .filter_map(|carried| {
                carried
                    .history_pledge
                    .as_ref()
                    .zip(carried.original_binding.as_ref())
                    .map(|(pledge, header)| (pledge.claim, header.clone()))
            })
            .collect();
        let producer_custody_delegations = objects
            .iter()
            .filter_map(|carried| {
                carried
                    .history_pledge
                    .as_ref()
                    .zip(carried.custody_delegation.as_ref())
                    .map(|(pledge, delegation)| (pledge.claim, delegation.clone()))
            })
            .collect();
        // Decisions 7/8: every lifecycle object meets its own validator
        // before the transition folds it — and an object that FAILS is
        // dropped rather than fatal to the block that accepted it.
        //
        // **Audit M-01's other half.** This used to disqualify the block.
        // Admission on the 0x4b band is purely stateless (decode, version,
        // may-ride), so a transaction carrying a stateful lie — a bad
        // signature, a claim that does not exist — relays and mines freely;
        // the first honest block to accept it lost its candidacy, and the
        // transaction stayed in the acceptance set for the next candidate
        // to die on. One ~100-byte transaction, one ordinary fee, and the
        // chain stops. The module beside this one states the rule already:
        // "a walk that could panic or reject on a peer-supplied payload
        // would be a remote denial of service wearing a consensus rule's
        // clothes" — it just stopped at the malformed carrier and left
        // every stateful check fatal.
        //
        // Dropping is deterministic: the verdict is a pure function of
        // (state, params, point, object), so every node drops the same
        // ones. A dropped object simply does not fold, which is what "the
        // transaction was invalid" ought to mean.
        // The class of this block's own attempt, read off its header, for the
        // rehearsal's reservation (2026-09-24 DoS audit review of #11). An
        // undecodable commitment reserves nothing — admission refuses it below.
        let own_attempt_class = Self::palw_own_attempt_class_of_header_v1(&header);
        //
        // **Except that a dropped market move's carrier stays accepted with
        // its MSK in the sink** (the 2026-09-23 Position route matrix,
        // P-B1), so past `palw_audit_2026_09_23` the filter also names what
        // each such carrier is owed and the transition pays it back.
        // Only the recognized, newly signed archive profile needs this
        // measurement. Decode real own-work bytes without treating them
        // as admitted work; bootstrap admission remains in its old order.
        let wants_native_input_context_v19 = state_params.claim_local_settlement_active_at(point.daa_score)
            && objects.iter().any(|c| {
                c.funded_original.as_ref().is_some_and(|p| {
                    kaspa_consensus_core::palw_court_original_escrow_v1::original_archive_profile_kind_v2(&p.envelope().funding.terms)
                        .is_some()
                })
            });
        let stored_non_daa;
        let merged_non_daa = if let Some(template) = manifest.as_ref().and_then(|input| input.template) {
            template.non_daa
        } else {
            use crate::model::stores::daa::DaaStoreReader;
            let raw = self.daa_excluded_store.get_mergeset_non_daa(current).map_err(|_| {
                PalwLocalTransitionFaultV1::Context(
                    kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
                )
            })?;
            stored_non_daa = self.palw_exec_v2_non_daa(&raw, &ctx.ghostdag_data, pov_daa_score);
            &stored_non_daa
        };
        let mut native_original_merged_v19 = None;
        let mut native_original_receipt_v19 = None;
        let native_original_context_v19 = if wants_native_input_context_v19 {
            let (merged_owned, merged_preskips) =
                self.palw_v2_merged_works(&ctx.ghostdag_data, state, state_params, merged_non_daa, &point)?;
            let (context, receipt) = self.palw_native_original_input_context_for_header_v19(
                state,
                state_params,
                &point,
                header.as_ref(),
                ctx.selected_parent(),
                &objects,
                &merged_owned,
            )?;
            native_original_merged_v19 = Some((merged_owned, merged_preskips));
            native_original_receipt_v19 = Some(receipt);
            context
        } else {
            None
        };
        // Retain physical dispositions only for a candidate carrying the
        // new protected companion. Old profiles keep their existing walk.
        // The full-basis check below runs only if one survives admission.
        let record_protected_physical_v3 = manifest.is_some()
            || self.palw_lifecycle_kind_fences.tokenizer_registration_v1_at(point.daa_score).is_some()
                && objects
                    .iter()
                    .any(|carried| carried.funded_original.as_ref().is_some_and(|pair| pair.public_companion_v3().is_some()));
        let manifest_selection = manifest
            .as_mut()
            .map(|input| {
                palw_accept::NativeManifestRequestSelectionV3::new_v3(
                    ctx,
                    input.maximum_archive_heap,
                    input.maximum_archive_work,
                    &mut *input.quote,
                )
            })
            .transpose()?;
        let mut accepted_batch = Vec::new();
        let mut tokenizer = palw_accept::TokenizerRegistrationAcceptanceV1 { candidates: tokenizer_candidates, ..Default::default() };
        let (
            (
                objects,
                folded,
                carrier_refunds,
                tir_public_originals,
                tir_funded_originals,
                fp_answers,
                prefix_dependencies,
                carrier_input_spends,
            ),
            assignment_rehearsal,
            mut selected_manifest,
        ) = tokenizer_registration_v1::with_prefix_scope_v1(tokenizer_prefix, || {
            self.palw_v2_accepted_objects_and_raw_assignments_v3(
                state,
                state_params,
                &point,
                objects,
                current,
                own_attempt_class,
                &ctx.palw_round_fee_incomes,
                &ctx.palw_escrow_settlements,
                native_original_context_v19.as_deref(),
                assignment_raw_actions,
                &ctx.accepted_tx_ids,
                if projecting {
                    palw_accept::NativeAssignmentRehearsalModeV2::ProjectJointFees
                } else {
                    palw_accept::NativeAssignmentRehearsalModeV2::Observed
                },
                &batch_candidates,
                &mut accepted_batch,
                &mut tokenizer,
                record_protected_physical_v3,
                manifest_selection,
            )
        })?;
        if tokenizer.fees != ctx.palw_tokenizer_fees_v1 {
            return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                "tokenizer fee reconciliation",
                "ordered full rehearsal differs from credited UTXO fees",
            )
            .into());
        }
        let palw_accept::NativeAssignmentRehearsalResultV2::Observed(observed_schedule) = assignment_rehearsal else {
            return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                "Assignment final fold",
                "fee preview cannot supply the observed schedule",
            )
            .into());
        };
        let (assignment_raw_schedule, protected_physical_v3, projected_relay) =
            observed_schedule.into_canonical_with_physical_v3(&accepted_batch, &tokenizer.accepted)?;
        // The singleton filter has consumed the measured basis. The
        // authoritative fold below builds its own actual basis anew.
        drop(native_original_context_v19);
        // Unit C step 4: a receipt-lane block spends a quantum, and its
        // right to do so is a DRAW — so the beacon it draws against is
        // derived from this candidate's own chain, never read off the
        // spending block. A block that supplied its own beacon would be
        // choosing the randomness that decides whether it wins.
        // **ADR-0064, and it is off unless a fence says otherwise.** With
        // `palw_bootstrap_activation` unset — every shipped preset — this
        // is `None` and the behaviour is byte-identical to before the
        // field existed. Armed, the block's own attempt may name a bond
        // this block's own accepted objects register — one chain block
        // earlier than before, for a producer joining a LIVE chain. It
        // does NOT restart a stopped one: this block's own body is not in
        // this set (a block's transactions are accepted by a later block),
        // so the registration still has to arrive in somebody else's
        // block. See ADR-0064's correction.
        let bootstrap = self.palw_bootstrap_activation.filter(|fence| fence.is_active(point.daa_score)).map(|_| &*folded);
        use native_original_input_v19::PalwOwnAttemptAdmissionV2;
        let own_attempt_admission = self.palw_resolve_own_attempt_v2(&header, state, state_params, &point, bootstrap)?;
        let attempt = match &own_attempt_admission {
            PalwOwnAttemptAdmissionV2::Attempt(envelope) => Some(envelope),
            PalwOwnAttemptAdmissionV2::NoAttempt => None,
            PalwOwnAttemptAdmissionV2::CeilingBinder { envelope, refusal } => {
                if !projecting {
                    info!(
                        "Block {} anchors at its bond's exposure ceiling ({}): it binds the claims due at \
                     it and its own attempt carries no claim (palw_anchor_at_ceiling)",
                        current, refusal
                    );
                }
                Some(envelope)
            }
            PalwOwnAttemptAdmissionV2::BaseRecovery { refusal } => {
                // RFC-0041 keeps the declared worker share, with no
                // claim or bond for the placeholder outpoint.
                if !projecting {
                    info!(
                        "Block {} stands as BASE coinbase recovery ({refusal}): no claim, no bond, the existing idle-floor subsidy",
                        current
                    );
                }
                None
            }
            PalwOwnAttemptAdmissionV2::Rejected { refusal } => {
                return Ok(PalwCandidateResultV3::Refused { stage: "admission", why: refusal.to_string() });
            }
        };
        // Admission owns its envelope now. Release the legacy
        // rehearsal and its construction lease before a protected
        // full-prefix replay allocates another working state.
        drop(folded);
        let receipt_spend = match native_original_receipt_v19.take().unwrap_or_else(|| {
            self.palw_v2_check_receipt_spend_with_parent_v3(&header, state, state_params, &point, || Ok(ctx.selected_parent()))
        }) {
            Ok(spend) => spend,
            Err(fp_error) => {
                return Ok(PalwCandidateResultV3::Refused { stage: "receipt spend", why: fp_error.to_string() });
            }
        };
        // **The block's own work, carried into the fold.**
        //
        // This was `None`, unconditionally, and it is the reason a V2
        // network could not mint weight: the two checks above admitted an
        // attempt or a spend and then threw the object away, so the
        // transition folded a block that — as far as the state machine
        // could tell — had done no work at all. `apply_attempt` never ran,
        // so no claim was ever created; `apply_receipt_spend` never ran,
        // so no certified quantum was ever burned and `safe_weight` never
        // moved off zero. Every downstream mechanism (panel binding,
        // licensing, courts, payouts, the safe frontier, fork choice)
        // reads claims, and there were none.
        //
        // At most one arm can be `Some`: the lane is chosen by the
        // header's single declared `pow_algo_id`, and the two constants
        // differ. The match is written total anyway rather than asserted,
        // because a panic here would be a remote one.
        let (work, own_execution_key, own_job_anchor) = self.palw_native_own_work_v1(header.as_ref(), attempt, receipt_spend.as_ref());
        // ADR-0058: the anticone's work rides this block's transition.
        // Assembled against the walk state (the parent's); the transition
        // re-runs the stateful admission per work against its LIVE fold
        // state, so order inside the block cannot over-commit a budget.
        let (merged_owned, merged_preskips) = if let Some(prepared) = native_original_merged_v19.take() {
            prepared
        } else {
            let (merged_owned, merged_preskips) =
                self.palw_v2_merged_works(&ctx.ghostdag_data, state, state_params, merged_non_daa, &point)?;
            (merged_owned, merged_preskips)
        };
        let (merged_refs, _merged_refs_owner) = self.palw_native_merged_work_refs_v1(&merged_owned)?;
        use kaspa_consensus_core::{
            palw_native_resource_v1::{NativeCoverageV1, reserve_native_requests_v1},
            palw_positive_certification_v2::{
                ready_positive_certification_case_v2, ready_positive_certification_subjects_v2,
                undrawn_positive_certification_case_v2, undrawn_positive_certification_subjects_v2,
            },
            palw_state_v2::{NativeAssignedCaseBackingV2, NativePositiveDecisionHistoryV2},
        };
        let unavailable = |why| NativeCoverageV1::unsupported::<Self>("Gate C selected-parent history", why);
        let scheduled = || {
            undrawn_positive_certification_subjects_v2(state, point.daa_score)
                .map(|subject| (subject, true))
                .chain(ready_positive_certification_subjects_v2(state, point.daa_score).map(|subject| (subject, false)))
        };
        let count = scheduled().count();
        let mut histories = Vec::new();
        let _histories_owner = if count == 0 {
            None
        } else {
            let unit = size_of::<NativePositiveDecisionHistoryV2>()
                + size_of::<kaspa_consensus_core::palw_state_v2::NativeCommittedAssignedCaseV2>()
                + 2 * size_of::<usize>();
            let bytes = count.checked_mul(unit).ok_or_else(|| unavailable("history list size overflow"))?;
            let owner = reserve_native_requests_v1(bytes as u64)?.ok_or_else(|| unavailable("history list capacity unavailable"))?;
            histories.try_reserve_exact(count).map_err(|_| unavailable("history list allocation refused"))?;
            for (subject, needs_draw) in scheduled() {
                let Some((case, result)) = (if needs_draw {
                    undrawn_positive_certification_case_v2(state, subject)
                } else {
                    ready_positive_certification_case_v2(state, subject, point.daa_score)
                })
                .map_err(|error| match error {
                    kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeResource(why) => why,
                    _ => unavailable("rooted certification subject unavailable"),
                })?
                else {
                    continue;
                };
                let Some(NativeAssignedCaseBackingV2::Committed(backing)) = self
                    .prepare_committed_case_history_v2(ctx.ghostdag_data.selected_parent, state, (case, Some(result)))
                    .map_err(|_| unavailable("committed Case archive unavailable"))?
                else {
                    return Err(unavailable("committed Case archive absent").into());
                };
                histories.push(NativePositiveDecisionHistoryV2(std::sync::Arc::new(backing)));
            }
            Some(owner)
        };
        // Declared before the immutable overlays: they drop before the
        // Manifest construction lease on every normal/error return.
        let attached_manifest;
        let transition_extras = prefix_dependencies.attach_v1(&fp_answers, {
            let mut extras = self.palw_transition_extras_for_objects(state, &point, &objects).into_legacy_extras();
            if kaspa_consensus_core::palw_state_v2::included_replay::protected_bound_panel_present_v1(state) {
                let draws = kaspa_consensus_core::palw_state_v2::included_replay::protected_draws_for_point_v1(state, &point);
                let mut approvals = Vec::new();
                let mut ordinal = 0u32;
                let replay_unavailable = |why| NativeCoverageV1::unsupported::<Self>("protected full-replay acceptance history", why);
                for mergeset in ctx.mergeset_acceptance_data.iter() {
                    if mergeset.accepted_transactions.is_empty() {
                        continue;
                    }
                    // An incomplete local store cannot turn an accepted
                    // approval into absence or renumber later carriers.
                    let block_txs = self
                        .block_transactions_store
                        .get(mergeset.block_hash)
                        .map_err(|_| replay_unavailable("accepted carrier block unavailable"))?;
                    for entry in &mergeset.accepted_transactions {
                        let tx = block_txs
                            .get(entry.index_within_block as usize)
                            .ok_or_else(|| replay_unavailable("accepted carrier index unavailable"))?;
                        if tx.id() != entry.transaction_id {
                            return Err(replay_unavailable("accepted carrier transaction id mismatch").into());
                        }
                        if let Some(approval) = kaspa_consensus_core::palw_state_v2::included_replay::included_replay_approval_v1(
                            state,
                            &point,
                            &draws,
                            mergeset.block_hash,
                            tx.id(),
                            ordinal,
                            &tx.payload,
                        ) {
                            approvals.push(approval);
                        }
                        ordinal = ordinal.checked_add(1).ok_or_else(|| replay_unavailable("accepted carrier ordinal overflow"))?;
                    }
                }
                if !draws.is_empty() {
                    extras.public_draws = draws;
                }
                if !approvals.is_empty() {
                    extras.public_replay_approvals = approvals;
                }
            }
            extras.history_custody_pledges = history_custody_pledges;
            extras.tir_public_originals = tir_public_originals;
            extras.tir_funded_originals = tir_funded_originals;
            extras.batch_court_carriers_v1 = accepted_batch;
            extras.tokenizer_registration_carriers_v1 = tokenizer.accepted;
            extras.tokenizer_registration_expected_fees_v1 = tokenizer.fees;
            extras.carrier_input_spends = carrier_input_spends;
            extras.public_original_bindings = public_original_bindings;
            extras.producer_custody_delegations = producer_custody_delegations;
            if state_params.exec_v2_active_at(point.daa_score) || state_params.monetary_material_at_v1(point.daa_score).is_some() {
                if attempt.is_some()
                    && let Ok(bytes) = borsh::to_vec(header.as_ref())
                {
                    extras.exec_v2_origin_headers.insert(current, bytes);
                }
                for owned in &merged_owned {
                    if let PalwMergedOwnedWorkV1::Attempt(source, _, _, _, _, origin_header) = owned
                        && let Ok(bytes) = borsh::to_vec(origin_header.as_ref())
                    {
                        extras.exec_v2_origin_headers.insert(*source, bytes);
                    }
                }
            }
            if projecting {
                extras.evm_actions = manifest.as_mut().and_then(|input| input.projected_evm_actions.take()).ok_or_else(|| {
                    kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                        "Manifest fee projection",
                        "disposable EVM market projection is absent",
                    )
                })?;
            } else if let Some(staged) = ctx.evm_staged.as_ref() {
                extras.evm_actions = staged.result.market_actions.clone();
            }
            // P-B1: what the filter above refused and owes back. Empty
            // below `palw_audit_2026_09_23`, and the fold checks again.
            extras.carrier_market_refunds = carrier_refunds;
            // ADR-0125: the permits this block accepted, as decided above.
            if let Some(verdicts) = ctx.palw_round_verdicts.as_ref() {
                extras.round_permit_uses = verdicts.uses.clone();
            }
            extras.round_fee_incomes = ctx.palw_round_fee_incomes.clone();
            // G14 lane escrow-rfc08: the qualified escrow spends UTXO acceptance admitted, settled by the fold
            // in this block's rooted delta (committed with the UTXO diff in one WriteBatch).
            extras.funded_escrow_settlements = ctx.palw_escrow_settlements.clone();
            // Raw op12 still derives settlement at the executed prefix.
            // Compare that outcome with what UTXO acceptance debited.
            extras.assignment_settlements = std::mem::take(&mut ctx.palw_assignment_settlements);
            extras.positive_assignment_actions = assignment_raw_schedule;
            extras.positive_decision_histories = histories;
            // RFC-0008 v2: and the EXEC blocks this block's anchor covered — the slice carriers (each
            // re-judged by the fold against its own running state) and the covered set itself.
            if let Some(resolved) = exec_resolved {
                let (anchor, slices) = self.palw_exec_v2_fold_inputs(resolved, point.daa_score);
                extras.exec_v2_anchor = Some(anchor);
                extras.exec_v2_covered = slices;
            }
            // ADR-0152 v3.1 J-1: the anchor this block's own attempt answers.
            extras.own_job_anchor = own_job_anchor;
            // **ADR-0165 / ADR-0125 semantic amendment: the genuine reds of this block's mergeset** — the floor state machine reads a merged
            // attempt's colour from them (a RED REAL attempt never extends a Probe or Normal).
            extras.merged_reds = ctx
                .ghostdag_data
                .classify_palw_mergeset_v1(|member| self.ghostdag_manager.is_round_block(member))
                .genuine_reds()
                .iter()
                .copied()
                .collect();
            // **Lane PA (`palw_audit_1004_v1`, RF-3 / P-F5): the beacon** a draw reads in place of this block's own hash.
            extras.audit_1004_draw_seed_source =
                self.palw_audit_1004_draw_seed_source(ctx.ghostdag_data.selected_parent, point.daa_score);
            attached_manifest = selected_manifest
                .take()
                .map(|selected| selected.attach_inputs_v3(state.state_root(), &point, &mut extras, !projecting))
                .transpose()?;
            extras
        })?;
        // A protected original must be rechecked against the complete
        // basis that the authoritative fold below will use: actual own
        // and merged work, all accepted objects and the rebuilt extras.
        // This single pass checks the selected list. It is not a fixed-
        // point selector: a mismatch must never silently become approval.
        if manifest.is_some() || transition_extras.tir_funded_originals.values().any(|pair| pair.public_companion_v3().is_some()) {
            use kaspa_consensus_core::palw_state_v2::{
                PalwOrderedReplayBudgetV1, PalwOrderedReplayLimitsV1, PalwOrderedReplayRulesV1,
            };
            let unavailable = |why| NativeCoverageV1::unsupported::<Self>("protected original ordered admission", why);
            let ledger =
                protected_physical_v3.ok_or_else(|| unavailable("accepted protected original has no physical candidate ledger"))?;
            let candidate_visits = ledger
                .event_count_v3()
                .checked_add(transition_extras.positive_assignment_actions.len())
                .and_then(|count| u64::try_from(count).ok())
                .ok_or_else(|| unavailable("physical replay visit count overflow"))?;
            // Only the legacy single-pass lane constructs a local budget.
            // Manifest callers retain one allowance across every candidate.
            let mut legacy_replay_budget = if manifest.is_none() {
                Some(
                    PalwOrderedReplayBudgetV1::new_v1(
                        state,
                        state_params,
                        point,
                        PalwOrderedReplayRulesV1 {
                            unavailable_abstains: self.palw_unavailable_abstains_at(point.daa_score),
                            capability_bound: self.palw_capability_bound_at(point.daa_score),
                            uncertified_weightless: self.palw_uncertified_weightless_at(point.daa_score),
                            da_court: self.palw_da_court_at(point.daa_score),
                        },
                        PalwOrderedReplayLimitsV1 { passes: 1, candidate_visits },
                    )
                    .map_err(|why| match why {
                        kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeResource(why) => {
                            PalwLocalTransitionFaultV1::Resource(why)
                        }
                        kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeContextUnavailable(why) => {
                            PalwLocalTransitionFaultV1::Context(why)
                        }
                        _ => unavailable("physical replay budget cannot bind the accepting point").into(),
                    })?,
                )
            } else {
                None
            };
            // Re-decode from the same verified acceptance data and real fee
            // handoffs. Leased answers/prefix dependencies are not cloned
            // without their native ownership or borrowed from another tx.
            let (replay_carried, _, _) = self.palw_v2_objects_of_block(
                &ctx.mergeset_acceptance_data,
                state,
                current,
                header.daa_score,
                &ctx.palw_v2_accepted_tx_fees,
                &ctx.palw_v2_rent_available_fees,
                &ctx.palw_batch_handoffs_v1,
                &ctx.palw_tokenizer_handoffs_v1,
                &ctx.palw_tfp1_history_seeds_v1,
            )?;
            let (replayed, projected) =
                tokenizer_registration_v1::with_prefix_scope_v1(tokenizer_prefix, || -> Result<_, PalwLocalTransitionFaultV1> {
                    if let Some(input) = manifest.as_mut() {
                        let selected = attached_manifest.as_ref().ok_or_else(|| unavailable("Manifest selection lost its owner"))?;
                        match input.mode {
                            NativeManifestCandidateModeV3::ProjectFees | NativeManifestCandidateModeV3::ProjectTemplateFees => {
                                let (next, fees) = self.palw_project_observed_manifest_fees_v3(
                                    input.replay_budget,
                                    state,
                                    state_params,
                                    &point,
                                    &objects,
                                    work,
                                    &merged_refs,
                                    own_execution_key,
                                    &transition_extras,
                                    selected,
                                    ledger,
                                    replay_carried,
                                    own_attempt_class,
                                    &mut *input.quote,
                                )?;
                                Ok((next, Some(fees)))
                            }
                            NativeManifestCandidateModeV3::Strict => {
                                selected.check_actual_projection_v3(
                                    ctx.palw_manifest_requests_v3
                                        .as_ref()
                                        .ok_or_else(|| unavailable("actual Manifest collection is absent"))?,
                                    &transition_extras,
                                )?;
                                let preparation = selected.replay_preparation_v3(&mut *input.quote);
                                let next = self.palw_recheck_observed_prefix_v3(
                                    input.replay_budget,
                                    state,
                                    state_params,
                                    &point,
                                    &objects,
                                    work,
                                    &merged_refs,
                                    own_execution_key,
                                    &transition_extras,
                                    ledger,
                                    replay_carried,
                                    own_attempt_class,
                                    Some(preparation),
                                )?;
                                Ok((next, None))
                            }
                        }
                    } else {
                        let next = self.palw_recheck_observed_prefix_v3(
                            legacy_replay_budget.as_mut().expect("legacy replay has its one-pass budget"),
                            state,
                            state_params,
                            &point,
                            &objects,
                            work,
                            &merged_refs,
                            own_execution_key,
                            &transition_extras,
                            ledger,
                            replay_carried,
                            own_attempt_class,
                            None,
                        )?;
                        Ok((next, None))
                    }
                })?;
            let replay_bootstrap = self.palw_bootstrap_activation.filter(|fence| fence.is_active(point.daa_score)).map(|_| &*replayed);
            let replay_admission = self.palw_resolve_own_attempt_v2(&header, state, state_params, &point, replay_bootstrap)?;
            if replay_admission != own_attempt_admission {
                return Err(unavailable("own work changed under the complete accepted prefix; reselection required").into());
            }
            if let Some(fees) = projected {
                // Projection consumes a disposable candidate. The driver
                // must rebuild actual UTXO acceptance before strict preparation;
                // it must not reuse the consumed raw settlement/handoff inputs.
                let relay = projected_relay.ok_or_else(|| unavailable("joint projection lost its selected relay plan"))?;
                if !relay.matches_projection_v3(&fees.relay) {
                    return Err(unavailable("complete prefix changed relay fees; fresh joint selection required").into());
                }
                let manifest_matches = ctx
                    .palw_manifest_requests_v3
                    .as_ref()
                    .ok_or_else(|| unavailable("projection lost actual Manifest collection"))?
                    .check_selected_projection_v3(&fees.manifest)
                    .is_ok();
                let relay_matches = transition_extras.positive_assignment_actions.iter().all(|scheduled| {
                    let observed = scheduled.action.relay_carrier_fees_v2();
                    match fees.relay.get(&scheduled.action.transaction().id()) {
                        Some(projected) => observed == Some(*projected),
                        None => observed.is_none_or(|fee| fee.relay_charge() == 0),
                    }
                });
                return Ok(PalwCandidateResultV3::Projected {
                    fees: fees.manifest,
                    relay,
                    fees_match_actual: manifest_matches && relay_matches,
                });
            }
            if let Some(selected) = attached_manifest.as_ref() {
                // The complete strict prefix above has checked the fresh map,
                // including current authority at every physical position.
                let pass = ctx
                    .palw_manifest_requests_v3
                    .as_mut()
                    .ok_or_else(|| unavailable("strict replay lost actual Manifest collection"))?;
                selected.reconcile_actual_fees_v3(pass, &transition_extras)?;
                match self.verify_expected_utxo_state_with_parent_v3(
                    ctx,
                    selected_parent_utxo_view,
                    selected_parent_bond_view,
                    header,
                    Some(state),
                ) {
                    Ok(()) => {}
                    Err(RuleError::LocalNativeCapacityUnavailable(why)) => return Err(why.into()),
                    Err(RuleError::LocalNativeStateContextUnavailable(why)) => return Err(PalwLocalTransitionFaultV1::Context(why)),
                    Err(why) => return Ok(PalwCandidateResultV3::Refused { stage: "UTXO", why: why.to_string() }),
                }
            }
            // Rehearsal owns no persistent delta. Only the successful
            // authoritative fold below may publish state and UTXO effects.
        }
        let selected_parent = ctx.ghostdag_data.selected_parent;
        let live_case_cap = self
            .palw_positive_assignment_params_v2
            .as_ref()
            .and_then(|params| params.palw_positive_assignment_v2_at(point.daa_score))
            .map_or(0, |fence| fence.policy.live.global as usize);
        let mut load_positive_case = |case, result| {
            use kaspa_consensus_core::{
                palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable, palw_state_v2::PalwStateV2Error,
            };
            self.prepare_committed_case_history_v2(selected_parent, state, (case, Some(result)))
                .map_err(|error| match error {
                    positive_assignment_archive::NativeAssignmentArchiveErrorV2::Resource(why) => {
                        PalwStateV2Error::NativeResource(why)
                    }
                    _ => PalwStateV2Error::NativeContextUnavailable(HistoryUnavailable),
                })?
                .ok_or(PalwStateV2Error::NativeContextUnavailable(HistoryUnavailable))
        };
        // Publish observations only from the authoritative fold,
        // after every provisional admission replay has been dropped.
        let court_observation_guard = kaspa_consensus_core::palw_court_observation_v1::CourtObservationGuardV1::enter(current);
        let mut run_fold = |extras: &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1| {
            tokenizer_registration_v1::with_prefix_scope_v1(tokenizer_prefix, || {
                kaspa_consensus_core::palw_state_v2::apply_palw_transition_retained_v8(
                    state,
                    state_params,
                    self.palw_admission_params_v2.as_ref(),
                    &point,
                    &objects,
                    work,
                    &merged_refs,
                    own_execution_key,
                    self.palw_unavailable_abstains_at(point.daa_score),
                    self.palw_capability_bound_at(point.daa_score),
                    // ADR-0069 Decision 7, at this BLOCK's DAA. `false` on every
                    // shipped preset, where the fold is byte-identical.
                    self.palw_uncertified_weightless_at(point.daa_score),
                    self.palw_da_court_at(point.daa_score),
                    // ADR-0088 Decision 11 and ADR-0089 Decision 6, at this BLOCK's
                    // DAA: the fences, and the actions this block's EVM step queued.
                    extras,
                    kaspa_consensus_core::palw_state_v2::PositiveCaseReplayInputV2 {
                        selected_parent,
                        live_case_cap,
                        load: &mut load_positive_case,
                    },
                    None,
                )
            })
        };
        let outcome = run_fold(&transition_extras);
        match outcome {
            Ok((next, delta, merged_skips, assignment_materials)) => {
                let observation = court_observation_guard.finish().map(|outcomes| {
                    kaspa_consensus_core::palw_court_observation_v1::CommittedCourtObservationV1 {
                        block: current,
                        post_state_root: next.state_root(),
                        daa: point.daa_score,
                        outcomes,
                    }
                });
                // A skipped merged work is the block standing while a
                // piece of its anticone is refused — say which and why,
                // because a silent skip of real work is the defect class
                // ADR-0058 exists to close.
                for (blue, why) in merged_preskips.iter().chain(merged_skips.iter()) {
                    info!("PALW: merged blue {blue} carried work this chain point refused (the accepting block stands): {why}");
                }
                // `palw_model_court_window`: a class registered past the fence commits its own court
                // window — say which, once per registration, so an operator (and the flag-day drill)
                // can read what every node derived. Empty below the fence.
                for entry in &delta.entries {
                    if let kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::ClassCourtWindow {
                        key,
                        old: None,
                        new: Some(window),
                    } = entry
                    {
                        info!(
                            "PALW model court window: class {key} registered at DAA {} commits a {window}-DAA court window (the network window is {})",
                            point.daa_score,
                            state_params.window_court()
                        );
                    }
                }
                // **ADR-0165: the floor state moved** — one `[palw-floor-state]` line per chain block whose
                // fold changed it (idle/probe/normal, with the DAA and the numbers it is made of), so the
                // drill and an operator read the machine without a debugger. Empty below the fence.
                if let Some((from, to)) = kaspa_consensus_core::palw_real_share_v1::palw_floor_state_change_v1(&delta.entries) {
                    info!(
                        "{}",
                        kaspa_consensus_core::palw_real_share_v1::palw_floor_state_log_line_v1(&current, point.daa_score, &from, &to)
                    );
                }
                // The construction ticket moves with the state; no
                // post-construction reservation can authorize its clone.
                let next_root = next.state_root();
                // Launch blockers §8: say it out loud. A voided claim's
                // escrow is burned by don't-mint (Decision 10), and a
                // network whose panels never bind burns the whole worker
                // carve of every block while looking exactly like one that
                // pays it.
                let destroyed = kaspa_consensus_core::palw_state_v2::palw_escrow_destroyed_by_delta_v2(&delta);
                if destroyed > 0 {
                    warn!(
                        "PALW: block {current} voided claims holding {destroyed} sompi of escrowed worker reward; that value is destroyed, not paid (ADR-0042 Decision 10)"
                    );
                }
                let (delta, delta_lease) = delta.into_parts_v1();
                // Persist only declarations supported by this successful
                // full fold, together with its UTXO and PALW delta.
                let class_carriages =
                    class_carriage_commit_v1::prepare_v1(&objects, &delta, transition_extras.model_immutable_active)?;
                // Keep observed fee evidence even when no rows were written.
                let relay_carriers = transition_extras.into_assignment_actions_v2();
                Ok(PalwCandidateResultV3::Prepared(PreparedPalwCandidateV3 {
                    next,
                    observation,
                    staged: PalwUtxoCommitV2 {
                        root: next_root,
                        delta,
                        _native_delta_lease: delta_lease,
                        assignment_materials,
                        relay_carriers,
                        class_carriages,
                        genesis: None,
                    },
                }))
            }
            Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeResource(why)) => {
                return Err(why.into());
            }
            Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeContextUnavailable(why)) => {
                return Err(PalwLocalTransitionFaultV1::Context(why));
            }
            Err(palw_error) => Ok(PalwCandidateResultV3::Refused { stage: "state", why: palw_error.to_string() }),
        }
    }
}
