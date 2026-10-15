//! **`VirtualStateProcessor`: palw accept methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl VirtualStateProcessor {
    pub fn palw_producer_facts_v2_impl(
        &self,
        class_id: kaspa_hashes::Hash64,
        bond: Option<kaspa_consensus_core::tx::TransactionOutpoint>,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let admission = self.palw_admission_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let virtual_read = self.virtual_stores.read();
        let candidate_daa = virtual_read.state.get().ok()?.daa_score;
        drop(virtual_read);
        let budget_fences = self.palw_epoch_budget_fences_at(candidate_daa);
        let mut facts = kaspa_consensus_core::palw_producer_v2::palw_producer_facts_v4(
            &state,
            state_params,
            admission,
            chain_point,
            candidate_daa,
            class_id,
            bond.map(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2).as_ref(),
            kaspa_consensus_core::palw_admission_v2::palw_work_lottery_floor_v1(
                &state,
                budget_fences.work_target_floor,
                budget_fences.single_lottery,
            ),
            // ADR-0149: the height the admission compares the candidate against, and the floor's
            // draw it prices the floor on before the registry has written its row.
            budget_fences.canonical_work_daa,
            budget_fences.base_known_draw,
            budget_fences.audit_2026_09_23_active,
            // Option A: the escrow the candidate block's own claim would carry — this block's subsidy
            // under the carve resolved at its DAA, the pair the fold and the ceiling price it from.
            kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(
                state_params,
                self.coinbase_manager.calc_block_subsidy(candidate_daa),
                budget_fences.escrow_carve,
            ),
            // ADR-0152 (S-SPEC §2, P6): the RAW second-clock depth at the candidate, so
            // `committed` reads live locks at the escaped depth exactly as admission and the fold do.
            self.palw_settled_anchor_depth_at(candidate_daa),
        )?;
        // Lane V02 (post-launch, review MEDIUM): past `palw_final_lock_full_collateral` the facts'
        // accuser ledger counts an open held dissection at its charge, as the fold and item 8 do.
        kaspa_consensus_core::palw_producer_v2::palw_producer_facts_apply_held_ledger_v1(
            &mut facts,
            &state,
            state_params,
            bond.map(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2).as_ref(),
            candidate_daa,
            budget_fences.held_accuser_charge_active,
        );
        // The producer reads the same parent snapshot as admission. At a crossing block the
        // snapshot still carries the closed epoch's table, so the boundary fence must derive the
        // candidate epoch's budget here too; otherwise the producer would hold a non-floor block
        // that admission accepts. Recompute the release after installing that derived budget,
        // because the class's own share is part of the release formula.
        if budget_fences.boundary_budget_active && facts.epoch_budget_blocks == 0 && !facts.is_base_class {
            let epoch_index = candidate_daa / state_params.epoch_length();
            if let Some(budgets) = kaspa_consensus_core::palw_state_v2::palw_epoch_budgets_for_v2(&state, state_params, epoch_index)
                && let Some(budget) = budgets.budget_blocks.get(&class_id).copied()
            {
                facts.epoch_budget_blocks = budget;
                facts.epoch_budget_released = kaspa_consensus_core::palw_state_v2::palw_epoch_budget_release_v1(
                    &state,
                    state_params.epoch_length(),
                    candidate_daa,
                    &class_id,
                    budget,
                );
            }
        }
        // ADR-0123: the builder computes the release but cannot know whether it counts — it holds
        // no `Params`. This is the one path every producer and the RPC ask through, and it resolves
        // the fence at the SAME candidate score admission will judge the block at, so a producer
        // holds exactly when the chain would refuse and draws exactly when it would accept.
        facts.epoch_budget_release_armed = budget_fences.budget_release_active;
        // **The route-matrix audit's #7: the fold's own class gate, asked before an inference is
        // spent** — at the candidate's DAA under the fences the fold will read there (the block is
        // not built yet, so its header facts are the tip's; the gate reads none of them).
        let extras = self.palw_transition_extras_for(&kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: chain_point,
            daa_score: candidate_daa,
            blue_score: 0,
            subsidy: 0,
        });
        facts.class_admission_refusal =
            kaspa_consensus_core::palw_state_v2::palw_class_admits_claim_v1(&state, state_params, &extras, &class_id, candidate_daa)
                .err()
                .map(|refusal| refusal.to_string());
        // **Lane PA, C-F2: what each rider of this lead is demanded** — at the rider's share of the lead's subsidy, as the fold admits it.
        // Past the canonical-work height and where riders are armed only; otherwise the producer keeps the lead's pwu as before.
        if state_params.capacity_riders_active_at(candidate_daa)
            && budget_fences.canonical_work_daa.is_some_and(|height| candidate_daa >= height)
        {
            let lead_escrow = kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(
                state_params,
                self.coinbase_manager.calc_block_subsidy(candidate_daa),
                budget_fences.escrow_carve,
            );
            let state_work = state.work_target().map(|target| target.work).unwrap_or(0);
            for n in 1..=kaspa_consensus_core::palw_capacity_s567_v1::PALW_RIDERS_MAX_V1 {
                let Some(subsidy) = kaspa_consensus_core::palw_state_v2::palw_rider_floor_subsidy_v1(
                    state_params,
                    lead_escrow,
                    n,
                    budget_fences.escrow_carve,
                ) else {
                    break;
                };
                // The floor the fold hands the admission for a rider (`TransitionBuilder::work_target_floor` at the share's subsidy).
                let floor = self
                    .palw_work_target_floor_for(candidate_daa, subsidy)
                    .map(|floor| if budget_fences.single_lottery { floor.max(state_work) } else { floor });
                let fences = kaspa_consensus_core::palw_admission_v2::PalwEpochBudgetFencesV1 {
                    work_target_floor: floor,
                    canonical_work_daa: budget_fences.canonical_work_daa,
                    base_known_draw: budget_fences.base_known_draw,
                    ..Default::default()
                };
                match kaspa_consensus_core::palw_admission_v2::palw_attempt_canonical_pwu_v1(
                    &state,
                    state_params,
                    &class_id,
                    candidate_daa,
                    fences,
                ) {
                    Ok(pwu) => facts.rider_pwu.push(pwu),
                    Err(_) => break,
                }
            }
        }
        // **RFC-0002 Part II Proposal A (`palw_class_seating`): and the class is seated for the named bond**, the fold's
        // seating door asked before an inference is spent (`Ok` below the fence).
        if facts.class_admission_refusal.is_none()
            && let Some(outpoint) = bond
        {
            let key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint);
            facts.class_admission_refusal = kaspa_consensus_core::palw_state_v2::palw_class_seated_admits_v1(
                &state,
                state_params,
                &extras,
                &key,
                &class_id,
                candidate_daa,
            )
            .err()
            .map(|refusal| refusal.to_string());
        }
        // **ADR-0152 v3.1 T-2(a) (S-6): and the named bond's share of the class**, which the fold
        // asks right after the class gate and skips the block's own attempt on — the attempt's
        // inference spent and its worker carve withheld and burned (`palw_v2_skipped_own_attempt_carve`).
        // Past `palw_rcore_plus` only; below it the share is no rule and this reads `Ok`.
        if facts.class_admission_refusal.is_none()
            && let Some(outpoint) = bond
        {
            let key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint);
            facts.class_admission_refusal = kaspa_consensus_core::palw_state_v2::palw_bond_class_share_admits_v1(
                &state,
                state_params,
                &extras,
                &key,
                &class_id,
                candidate_daa,
            )
            .err()
            // ADR-0160 stage 4 (F-N): an unregistered bond mines the refused attempt — it registers it.
            .filter(|refusal| {
                !kaspa_consensus_core::palw_state_v2::palw_network_refusal_registers_v1(
                    &state,
                    state_params,
                    refusal,
                    &key,
                    &class_id,
                    candidate_daa,
                )
            })
            .map(|refusal| refusal.to_string());
        }
        // **Lane bind-deadlock (`palw_anchor_at_ceiling`): would the candidate be a binder?** The
        // walk's own conditions (`palw_v2_anchor_at_ceiling_binder_v1`), read at the tip for the
        // candidate's DAA: the fence and R-core+ in force there; past lane A's fence there, the named
        // bond is an operator's (the walk keeps an operator's own attempt only — a non-operator's
        // binder is disqualified, an inference wasted); and a claim `Provisional` with its slot at or
        // below the candidate's DAA — the slots the producer's own attempt anchors (the attempt lane
        // below lane A; past it an operator's attempt reaches its own DAA).
        let lane_a_admits_the_bond = match self.palw_operator_anchor.as_ref().filter(|rule| rule.active_at(candidate_daa)) {
            Some(rule) => bond.is_some_and(|outpoint| rule.operators().any(|(operator, _)| operator.0 == outpoint)),
            None => true,
        };
        facts.binder_due = self.palw_anchor_at_ceiling_at(candidate_daa)
            && lane_a_admits_the_bond
            && self.palw_rcore_plus_at(candidate_daa)
            && self.palw_panel_params_v2.as_ref().is_some_and(|panel| {
                !kaspa_consensus_core::palw_state_v2::palw_claims_provisional_past_their_anchor_slot_v1(
                    &state,
                    candidate_daa,
                    panel.anchor_delay(),
                )
                .is_empty()
            });
        // **ADR-0160 F-R (J-6): and, on the floor, the seats' capital and the bond's share of it**,
        // which the fold asks before the ceiling and skips the block's own attempt on — priced over a
        // template of the claim this block would carry (its weight term and its escrow, the facts'
        // own numbers). Past `palw_capacity_verify_room` only; below it this reads `Ok`.
        if facts.class_admission_refusal.is_none()
            && facts.is_base_class
            && state_params.capacity_room_active_at(candidate_daa)
            && let (Some(outpoint), Some(bond_facts)) = (bond, facts.bond.as_ref())
        {
            let escrow = kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(
                state_params,
                self.coinbase_manager.calc_block_subsidy(candidate_daa),
                budget_fences.escrow_carve,
            );
            // rcore/cap-s1: the facts' `claim_exposure` is F-W's capped weight term plus F-E's escrow slot
            // (`claim_escrow_term_v2`: `m_c` past F-E, option A's `E` below it), so the weight is what is left
            // after THAT slot — the one the template's commitment re-reads.
            let weight = bond_facts.claim_exposure.saturating_sub(state_params.claim_escrow_term_v2(
                candidate_daa,
                escrow,
                &state_params.base_class_id(),
            ));
            let key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint);
            facts.class_admission_refusal = kaspa_consensus_core::palw_state_v2::palw_floor_room_admits_v1(&state, state_params, &extras, &key, weight, escrow, candidate_daa)
                    .err()
                    // ADR-0160 stage 4 (F-N): an unregistered bond mines the refused attempt — it registers it.
                    .filter(|refusal| {
                        !kaspa_consensus_core::palw_state_v2::palw_network_refusal_registers_v1(
                            &state,
                            state_params,
                            refusal,
                            &key,
                            &class_id,
                            candidate_daa,
                        )
                    })
                    .map(|refusal| refusal.to_string());
        }
        // **ADR-0160 F-N (stage 4): and a network unit** — lane N's level and work-conserving share, the
        // fold's last check before its first write, over the same template. A refusal holds the producer
        // back only while its bond's demand for the class stands registered through the candidate DAA: an
        // unregistered bond mines the refused attempt anyway, because that refusal is what registers it
        // (ADR-0160 v3 §7.2 — registering costs a refused attempt block every `H_L`), and a registered bond
        // below its share is admitted whenever a unit is free. Past `palw_capacity_network_room` only.
        if facts.class_admission_refusal.is_none()
            && state_params.capacity_network_active_at(candidate_daa)
            && let (Some(outpoint), Some(bond_facts)) = (bond, facts.bond.as_ref())
        {
            let key = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint);
            let escrow = kaspa_consensus_core::palw_state_v2::palw_claim_escrow_v1(
                state_params,
                self.coinbase_manager.calc_block_subsidy(candidate_daa),
                budget_fences.escrow_carve,
            );
            let weight = bond_facts.claim_exposure.saturating_sub(state_params.claim_escrow_term_v2(candidate_daa, escrow, &class_id));
            facts.class_admission_refusal = kaspa_consensus_core::palw_state_v2::palw_network_room_admits_v1(
                &state,
                state_params,
                &extras,
                &key,
                &class_id,
                weight,
                escrow,
                candidate_daa,
            )
            .err()
            .filter(|refusal| {
                !kaspa_consensus_core::palw_state_v2::palw_network_refusal_registers_v1(
                    &state,
                    state_params,
                    refusal,
                    &key,
                    &class_id,
                    candidate_daa,
                )
            })
            .map(|refusal| refusal.to_string());
        }
        // The share and the tip's count beside the refusal — a read the producer adds its own
        // unmerged attempts to (node policy; the fold's question above is unchanged).
        facts.bond_class_share = bond.and_then(|outpoint| {
            kaspa_consensus_core::palw_state_v2::palw_bond_class_share_read_v1(
                &state,
                state_params,
                &extras,
                &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(outpoint),
                &class_id,
                candidate_daa,
            )
        });
        Some(facts)
    }

    /// **ADR-0042 Decisions 7 and 8's consumers: every lifecycle object is ADJUDICATED before it
    /// is folded.**
    ///
    /// `apply_palw_transition_v3` moves a claim's phase on the object's say-so — deliberately, its
    /// module boundary says so: signature verification and sortition are `palw_panel_v2`'s job,
    /// the court's verdict is `palw_court_v2`'s. Folding an object nobody validated would make the
    /// state machine a transcription service for whatever a block asserted, which is the fail-open
    /// shape the consumer-layer audit found ten of.
    ///
    /// So each object meets its own validator here, BEFORE the fold:
    ///
    /// * **`PanelBound`** — `validate_panel_bound_v2` re-derives the panel by sortition from the
    ///   candidate's own bond registry and compares seats EXACTLY. The anchor is not taken from
    ///   the object either: it is derived from this candidate's chain, the same rule the beacon
    ///   follows, because a producer that supplies its own anchor picks its own jury.
    /// * **`CourtOpened`** — `validate_court_opened_v2`: the claim is challengeable at this point
    ///   and the session id is the one its own parts derive.
    /// * **`CourtClosed`** — `adjudicate_court_close_v2` returns the ONLY verdict the proof
    ///   supports, and the object's announced verdict must equal it. An object that merely NAMES
    ///   a verdict is an accusation, not an adjudication.
    ///
    /// A network with no V2 bundle has no objects to check (extraction yields none), so this is
    /// `Ok(())` everywhere today.
    /// [`Self::palw_v2_validate_objects`] as a FILTER: the objects that passed, in order, with the
    /// rejected ones logged by their own reason. See the call site for why a rejection drops the
    /// object instead of the block.
    /// [`Self::palw_v2_accepted_objects`], reachable from the sibling test module. The filter's
    /// contract — "what it returns, the transition applies" — is only checkable by calling both.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_accepted_objects_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        block: BlockHash,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.palw_v2_accepted_objects(state, state_params, point, Self::unpriced_for_tests(objects), block, None).0
    }

    /// The same filter, with the state it folded to — ADR-0064's half. The bootstrap lookup reads
    /// bonds out of THIS, so a test that only saw the accepted objects could not tell the fixed
    /// behaviour ("the registry the transition will have") from the one it replaced ("the first
    /// `BondRegistered` in the list"), which differ exactly when a block registers and then
    /// touches the same bond.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_accepted_objects_and_state_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        block: BlockHash,
    ) -> (Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>, kaspa_consensus_core::palw_state_v2::PalwChainStateV2) {
        self.palw_v2_accepted_objects(state, state_params, point, Self::unpriced_for_tests(objects), block, None)
    }

    /// [`Self::palw_v2_accepted_objects`] with each object's carrier fee spelled out, so the
    /// ADR-0075 rent rules can be exercised from the sibling test module.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_accepted_priced_objects_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, u64)>,
        block: BlockHash,
    ) -> (Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>, kaspa_consensus_core::palw_state_v2::PalwChainStateV2) {
        let objects = objects
            .into_iter()
            .map(|(object, carrier_fee)| PalwCarriedObjectV1 { object, carrier_fee, history_pledge: None, original_binding: None, refund: None, unrefundable: None })
            .collect();
        self.palw_v2_accepted_objects(state, state_params, point, objects, block, None)
    }

    /// [`Self::palw_v2_accepted_objects_and_refunds`] with each object's refund spelled out — the
    /// 2026-09-23 Position route matrix, P-B1 — so the sibling test module can check the filter
    /// pays back exactly the carriers it refused.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_v2_accepted_refundable_objects_for_tests(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<(
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
            Option<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
        )>,
        block: BlockHash,
    ) -> (
        Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        Vec<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
    ) {
        let objects = objects
            .into_iter()
            .map(|(object, refund)| PalwCarriedObjectV1 { object, carrier_fee: PALW_RENT_UNPRICED, history_pledge: None, original_binding: None, refund, unrefundable: None })
            .collect();
        self.palw_v2_accepted_objects_and_refunds(state, state_params, point, objects, block, None, &[])
    }

    /// [`Self::palw_v2_accepted_objects_and_refunds`] without the refunds — every caller but the
    /// chain walk, which is the only one that hands refunds to the transition.
    #[cfg(test)]
    pub(super) fn palw_v2_accepted_objects(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<PalwCarriedObjectV1>,
        block: BlockHash,
        // The class of the block's own attempt (from its header), `None` for a block with none: past
        // `palw_audit_2026_09_23` the fold's step 3 holds room for it on the class gate, and the
        // rehearsal must refuse the commitments the fold will.
        own_attempt_class: Option<kaspa_hashes::Hash64>,
    ) -> (Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>, kaspa_consensus_core::palw_state_v2::PalwChainStateV2) {
        let (accepted, folded, _refunds) =
            self.palw_v2_accepted_objects_and_refunds(state, state_params, point, objects, block, own_attempt_class, &[]);
        (accepted, folded)
    }

    /// The acceptance filter, and — the 2026-09-23 Position route matrix, P-B1 — the refunds owed to
    /// the carrier-borne market moves it refused, in acceptance order.
    pub(super) fn palw_v2_accepted_objects_and_refunds(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<PalwCarriedObjectV1>,
        block: BlockHash,
        // The class of the block's own attempt (from its header), `None` for a block with none: past
        // `palw_audit_2026_09_23` the fold's step 3 holds room for it on the class gate, and the
        // rehearsal must refuse the commitments the fold will (2026-09-24 DoS audit review of #11).
        own_attempt_class: Option<kaspa_hashes::Hash64>,
        round_fee_incomes: &[kaspa_consensus_core::palw_bond_budget_v1::PalwRoundFeeIncomeV1],
    ) -> (
        Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        Vec<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
    ) {
        kaspa_consensus_core::palw_attempt_rules_v1::palw_with_prompt_root_block_memo_v2(point.block, || {
            self.palw_v2_accepted_objects_and_refunds_inner(
                state,
                state_params,
                point,
                objects,
                block,
                own_attempt_class,
                round_fee_incomes,
            )
        })
    }

    pub(super) fn palw_v2_accepted_objects_and_refunds_inner(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: Vec<PalwCarriedObjectV1>,
        block: BlockHash,
        // The class of the block's own attempt (from its header), `None` for a block with none: past
        // `palw_audit_2026_09_23` the fold's step 3 holds room for it on the class gate, and the
        // rehearsal must refuse the commitments the fold will (2026-09-24 DoS audit review of #11).
        own_attempt_class: Option<kaspa_hashes::Hash64>,
        round_fee_incomes: &[kaspa_consensus_core::palw_bond_budget_v1::PalwRoundFeeIncomeV1],
    ) -> (
        Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
        kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        Vec<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
    ) {
        // **RFC-0007 §I.6: watch the vertices go by** (past the fence only). Every vertex whose signature verifies under its seat's key
        // is remembered, taken by the fold or not, so a second root of one `(round, seat)` — which the fold drops as the round's second
        // vertex — becomes evidence for the node to carry. A vertex already seen is not verified again.
        if self.palw_vertex_at(point.daa_score) {
            let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                self.network_id_bytes.as_slice(),
                Some(self.genesis.hash),
            );
            let mut watch = self.palw_vertex_watch.lock().unwrap();
            for carried in &objects {
                let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::VerificationVertexV1 { vertex } = &carried.object
                else {
                    continue;
                };
                if watch.knows(vertex.round, &vertex.seat_bond, &vertex.leaves_root)
                    || kaspa_consensus_core::palw_vertex_v1::palw_vertex_shape_v1(vertex).is_err()
                {
                    continue;
                }
                if kaspa_consensus_core::palw_vertex_v1::palw_vertex_verify_signature_v1(
                    state,
                    network_domain,
                    &vertex.seat_bond,
                    vertex.round,
                    vertex.signed_daa,
                    vertex.leaves.len(),
                    vertex.leaves_root,
                    &vertex.signature,
                    Self::verify_mldsa87_with_context_bool,
                )
                .is_ok()
                {
                    watch.observe_verified(vertex);
                }
            }
        }
        // **Filtered SEQUENTIALLY, against the state each accepted object leaves behind.**
        //
        // Validating every object against the parent state alone is wrong in exactly the way the
        // transition is right: the transition applies them in order, so the second object naming a
        // claim meets the phase the first one moved it to. Two nodes independently assembling the
        // same quorum — which is the design, since one funded submitter per network suffices and
        // several may be funded — put two `ReceiptLicensed` for one claim in one block. Both passed
        // this filter (both were valid against the parent), the transition applied the first, and
        // the second was refused as `wrong phase for ReceiptLicensed` — killing an honest block for
        // carrying a duplicate of its own valid object.
        //
        // Measured on testnet-12: 175 blocks produced, 23 accepted, 74 disqualified, the chain's
        // DAA frozen at 103 while three hosts submitted correctly.
        //
        // Folding the state forward here makes the filter ask the question the transition will ask.
        // A duplicate is then dropped as an object — which is what `Ⅱ.5` decided a failing object
        // should be — and the block stands.
        // The fold is a REHEARSAL, and its chain point has to advance for the transition to accept
        // it at all: `apply_palw_transition_v3` demands a strictly increasing blue score, so
        // re-applying at the block's own point succeeds exactly once and then refuses everything
        // with `blue_score must strictly increase` — which silently dropped every object after the
        // first, licensed nothing, and left the chain producing blocks whose weight never moved.
        // (Measured before this line existed: 356 blocks, 72 submissions, zero licensed.)
        //
        // Each rehearsal step therefore gets its own synthetic point one blue score along. The
        // fold's job is to answer "what phase will this claim be in when the transition reaches
        // the next object", and that answer does not depend on the point's exact value — only on
        // the objects already applied.
        // **A-1 fix (mainnet audit 2026-09-11): rehearse the fold's step 3, not a whole-block
        // transition per object.** The real fold applies every object in step 3, then runs
        // activation, budgets and block/merged work ONCE. Rehearsing each object through a full
        // transition ran activation between objects, so a `ClassLaneCertified` for a class that
        // becomes `Active` in this very block was accepted here and refused by the real fold —
        // disqualifying the block on every node and halting the chain. `folded` is now the fold's
        // pre-object base (payout/settlement drain, sweeps, retarget/growth/reclamation, once), and
        // each accepted object advances it by one `apply_object` and nothing else.
        // **A-1/AC-SLOT are gated to the audit flag day (`palw_audit_2026_09_11`).** Below the
        // fence this filter reproduces the pre-audit behavior EXACTLY — it rehearses each object
        // through a whole-block transition at a synthetic point, and charges the court slot on the
        // acceptance check — so a build with the fence and one without fold every block before the
        // height identically. Past the fence it folds the transition's step 3 (A-1) and charges the
        // slot only after a move applies (AC-SLOT).
        let audit_active = self.palw_audit_2026_09_11_at(point.daa_score);
        // The old path's synthetic chain point; unused past the fence.
        let mut rehearsal = *point;
        let mut fee_extras = self.palw_transition_extras_for(point);
        fee_extras.round_fee_incomes = round_fee_incomes.to_vec();
        let mut folded = if audit_active {
            match kaspa_consensus_core::palw_state_v2::palw_v2_pre_object_base_v1(
                state,
                state_params,
                point,
                self.palw_unavailable_abstains_at(point.daa_score),
                self.palw_capability_bound_at(point.daa_score),
                self.palw_uncertified_weightless_at(point.daa_score),
                self.palw_da_court_at(point.daa_score),
                &fee_extras,
            ) {
                Ok(base) => base,
                // The pre-object steps are what the real fold runs before any object; if they error
                // the block is disqualified whatever it carries, so the filter accepts nothing.
                Err(why) => {
                    info!(
                        "Block {block}: no PALW object is accepted; the pre-object fold fails and the block will be disqualified: {why}"
                    );
                    return (Vec::new(), state.clone(), Vec::new());
                }
            }
        } else {
            state.clone()
        };
        let mut accepted = Vec::with_capacity(objects.len());
        let mut model_classes_registered_in_block = std::collections::BTreeSet::new();
        // **The 2026-09-23 Position route matrix, P-B1: a refused carrier buy or seed is owed its
        // MSK back.** Its carrier is already accepted and its sink already holds the payment, so a
        // drop here used to be a burn. Every drop path below ends the iteration without reaching
        // `accepted.push`, so "refused" is read the one way that cannot miss a path: an object that
        // carried a refund is owed it iff `accepted` is no longer after it than it was before it.
        // Only objects past `palw_audit_2026_09_23` carry one (`palw_v2_objects_of_block`).
        let mut refunds: Vec<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1> = Vec::new();
        let mut refund_owed: Option<PalwPendingCarrierRefundV1> = None;
        // **Refunds COUNT against `PALW_V2_MAX_PENDING_PAYOUTS`** (user decision on the P-B1
        // review). Each refund this filter owes takes one queue row at step 3′, after every object,
        // so its row is RESERVED here the moment it is owed: a later move is measured against the
        // queue plus the reserved rows, and cannot take the row a refund was promised. A refund the
        // queue has no room for is not owed (its MSK stays in the sink, logged); the node's mempool
        // and template refuse such a carrier, so an honest node never mines one.
        let mut refund_rows_reserved = 0usize;
        // Past the 2026-09-23 fence (on a chain past the 2026-09-11 audit, whose rehearsal folds
        // the real step 3 so `folded` counts every row an accepted move wrote) the queue is counted
        // EXACTLY: accepted moves' rows are in `folded`, owed refunds are reserved, and a refused
        // move promises nothing — the review's starvation, where a cheap refused buy took two
        // phantom rows from the valid moves after it, is gone. Below it, the old promise counter.
        //
        // **Past `palw_rcore_plus` the queue also grows at step 3d — after every object, 3′ and 3c —
        // by the matured vesting legs and reporter rewards it moves** (ADR-0152 V-4/V-7; phase2-plan
        // F4). Nothing here reserves for them, and nothing needs to: the count below is the queue as
        // the fold's step 3 sees it (`palw_v2_pre_object_base_v1` mirrors steps 1b–2, and past the
        // fence a `Final` in step 2 writes a row, not a payout — I-3), which is exactly what the
        // fold's own market room reads; 3d's moves are exempt from `PALW_V2_MAX_PENDING_PAYOUTS`
        // (M-10) and bounded instead by V-7's per-block budget, so a committed queue holds at most
        // the cap plus one drain. Were a fold path ever to write the queue BEFORE 3d, this rehearsal
        // would have to reserve those rows the way it reserves refunds. T29 pins the agreement with a
        // 1,016-row queue, a row maturing in the same block, and a same-block conviction.
        let exact_queue = audit_active && self.palw_audit_2026_09_23_at(point.daa_score);
        let mut certifications_graded = 0usize;
        let mut exec_origin_budget = kaspa_consensus_core::palw_work_slice_v2::PalwExecOriginBudgetV1::default();
        // ADR-0080 design A, W9: how many declared closes this block has already completed.
        let mut court_closes_completed = 0usize;
        // 2026-09-24 DoS audit #12 (b): bought class registrations this block has been charged for.
        let mut class_registrations_charged = 0usize;
        // RFC-0002 Phase F: whether this block has handed an IR class registration to the gate
        // (admission v10) — at most one is (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`).
        let mut tir_registration_gated = false;
        // ADR-0152 v3.1 addendum §4-bis.3: prompt ids charged for whole-prompt recomputations, and
        // the claims already charged (a claim is charged once per block).
        let mut heavy_prompt_ids_charged = 0u64;
        let mut heavy_prompt_claims: std::collections::BTreeSet<kaspa_hashes::Hash64> = std::collections::BTreeSet::new();
        // Review of #12: the registrant bonds whose charged registration the gate then refused in
        // this block. Each takes no further slot this block (see the charging site).
        let mut class_registrants_refused: std::collections::BTreeSet<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2> =
            std::collections::BTreeSet::new();
        // **ADR-0042 Decision 10 / ADR-0087 Decision 3 (mainnet audit 2026-09-06, M-10): the payout
        // rows this block's CARRIER-borne market moves have already promised.**
        //
        // Counted here, in transaction order, against the state each accepted object leaves behind —
        // the same rehearsal `PALW_CERTIFICATION_MAX_PER_BLOCK` and `PALW_COURT_CLOSE_MAX_PER_BLOCK`
        // are counted against — and the excess move is DROPPED with the block standing, exactly as
        // those two are. Invalidating instead would be audit M-01's shape: admission on the
        // lifecycle band is stateless, so a second market carrier relays and mines freely and honest
        // miners would be the ones producing the invalid blocks.
        //
        // Without it the fold would refuse the move with `ModelPayoutQueueFull` and take the whole
        // block with it. The rehearsal drops it first, so the fold's guard is the belt to this
        // brace and is unreachable on the carrier lane.
        let mut model_payout_rows_promised = 0usize;
        // **ADR-0075 SA-1/SA-2's fence, asked HERE and not only where the fees were resolved.**
        // `false` on every shipped preset, and then neither rent below can fire whatever a caller
        // passed for a carrier fee — so the filter's behaviour is byte-identical to before the
        // amendment rather than identical-if-the-caller-remembers.
        let rent_armed = self.palw_certification_rent_at(point.daa_score);
        // Armed only (see the accounting below): the fault vectors this block has already asked
        // the grader to walk, refused or accepted. Dormant it is never read and never written.
        let mut vectors_graded = 0usize;
        for carried in objects {
            Self::palw_v2_settle_carrier_refund(
                &mut refund_owed,
                accepted.len(),
                folded.pending_payouts_iter().count().saturating_add(model_payout_rows_promised),
                &mut refund_rows_reserved,
                &mut refunds,
                block,
            );
            let PalwCarriedObjectV1 { object, carrier_fee, history_pledge, original_binding, refund, unrefundable } = carried;
            let public_original_bindings = history_pledge.as_ref().zip(original_binding).map(|(pledge, header)| (pledge.claim, header))
                .into_iter().collect::<std::collections::BTreeMap<_, _>>();
            let history_custody_pledges = history_pledge.into_iter().map(|pledge| (pledge.claim, pledge)).collect::<std::collections::BTreeMap<_, _>>();
            refund_owed = (refund.is_some() || unrefundable.is_some()).then_some(PalwPendingCarrierRefundV1 {
                accepted_before: accepted.len(),
                refund,
                unrefundable,
            });
            // **RFC-0009 G-EXPIRY / G-RULESET: a signed-expiry envelope (tag 108) is replaced by the registration it wraps** — or dropped
            // by name, charged nothing, when its fence is not in force, it has expired, it names another ruleset or its signature
            // fails. Everything below then judges the wrapped registration exactly as if it had been carried bare.
            let object = if kaspa_consensus_core::palw_state_v2::palw_object_is_signed_registration_v1(&object) {
                match self.palw_signed_registration_unwrap(&folded, point.daa_score, &object) {
                    Ok(inner) => inner,
                    Err(why) => {
                        info!("Block {block}: a signed registration envelope was dropped, and the block stands: {why} (RFC-0009)");
                        continue;
                    }
                }
            } else {
                object
            };
            // **RFC-0002 Phase F: below `palw_tir_v1` an IR object is dropped by name before any rule
            // below reads it.** An older build cannot decode it and skips it (A-2) without charging it
            // a registration slot, the court's adjudication slot or any budget — so it is dropped
            // here, first, and charged nothing either. The gate's own arms and the fold refuse it too.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_tir_v1(&object) && !self.palw_tir_at(point.daa_score) {
                info!("Block {block}: an IR object was dropped by name below palw_tir_v1, and the block stands (RFC-0002 Phase F)");
                continue;
            }
            // **G14 lane D: below `palw_probabilistic_constraints_v1` a kernel route object is dropped by name**, first and charged
            // nothing, for the IR objects' reason above (an older build cannot decode it and skips it, A-2).
            if (kaspa_consensus_core::palw_state_v2::palw_object_is_kernel_route_v1(&object)
                || kaspa_consensus_core::palw_state_v2::palw_object_is_onboarding_v1(&object))
                && !self.palw_kernel_route_at(point.daa_score)
            {
                info!(
                    "Block {block}: a kernel route object was dropped by name below palw_probabilistic_constraints_v1, and the block stands (G14)"
                );
                continue;
            }
            // **Lane BUDGET: below `palw_model_bond_allocation_v1` a capital assignment (tag 140) is dropped by name**, first and charged
            // nothing (an older build cannot decode it and skips it, A-2).
            if kaspa_consensus_core::palw_state_v2::palw_object_is_bond_budget_v1(&object)
                && !self.palw_model_bond_allocation_at(point.daa_score)
            {
                info!(
                    "Block {block}: a capital assignment was dropped by name below palw_model_bond_allocation_v1, and the block stands (ADR-0177)"
                );
                continue;
            }
            // **Lane DA16: below `palw_provider_court_v1` a provider-court object (tags 150–153) is dropped by name**, first and charged
            // nothing, for the same reason (an older build cannot decode it and skips it, A-2).
            if kaspa_consensus_core::palw_state_v2::palw_object_is_provider_court_v1(&object)
                && !self.palw_provider_court_at(point.daa_score)
            {
                info!(
                    "Block {block}: a provider-court object was dropped by name below palw_provider_court_v1, and the block stands (DA16)"
                );
                continue;
            }
            // **Lane LG14-A: below `palw_legacy_public_filer_v1` a dispute object (tags 154–155) is dropped by name**, first and charged
            // nothing, for the same reason (the live int-12 build cannot decode it and skips it, A-2).
            if kaspa_consensus_core::palw_legacy_public_filer_v1::palw_object_is_legacy_dispute_v1(&object)
                && !self.palw_legacy_public_filer_at(point.daa_score)
            {
                info!(
                    "Block {block}: a dispute object was dropped by name below palw_legacy_public_filer_v1, and the block stands (LG14-A)"
                );
                continue;
            }
            // **Lane LG14-B: below `palw_legacy_held_da_v2` a legacy held object (tags 157–159) — or an int-12 object carrying its
            // appended unit — is dropped by name**, first and charged nothing, for the same reason (an older build cannot decode it and
            // skips it, A-2).
            if kaspa_consensus_core::palw_state_v2::palw_object_is_legacy_held_da_v2(&object)
                && !self.palw_legacy_held_da_v2_at(point.daa_score)
            {
                info!(
                    "Block {block}: a legacy held DA object was dropped by name below palw_legacy_held_da_v2, and the block stands (LG14-B)"
                );
                continue;
            }
            // **RFC-0003: below `palw_gen_v1` a generative object is dropped by name**, first and
            // charged nothing, for the IR objects' reason above (an older build skips it undecoded).
            if kaspa_consensus_core::palw_state_v2::palw_object_is_gen_v1(&object) && !self.palw_gen_at(point.daa_score) {
                info!("Block {block}: a generative object was dropped by name below palw_gen_v1, and the block stands (RFC-0003)");
                continue;
            }
            // **The task-head profile: below `palw_task_heads_v1` an object carrying a `Head` variant is dropped by name**, first and
            // charged nothing — the int-12 build cannot decode its bytes and skipped it at extraction (A-2).
            if kaspa_consensus_core::palw_task_heads_v1::palw_object_needs_task_heads_v1(&object)
                && !self.palw_task_heads_at(point.daa_score)
            {
                info!("Block {block}: a task-head object was dropped by name below palw_task_heads_v1, and the block stands (A-2)");
                continue;
            }
            // **ADR-0096 Decision 7: below `palw_fp_decode_constraint` a constrained decode close is dropped by name**, first
            // and charged nothing, for the generative objects' reason above.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_constrained_decode_v1(&object)
                && !self.palw_fp_decode_constraint_at(point.daa_score)
            {
                info!(
                    "Block {block}: a constrained decode close was dropped by name below palw_fp_decode_constraint, and the block stands (ADR-0096)"
                );
                continue;
            }
            // **RFC-0004 A6: below `palw_improvement_v1` an evaluation court move is dropped by name**, first and
            // charged nothing, for the generative objects' reason above.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_eval_v1(&object) && !self.palw_improvement_at(point.daa_score) {
                info!(
                    "Block {block}: an evaluation court object was dropped by name below palw_improvement_v1, and the block stands (RFC-0004)"
                );
                continue;
            }
            // **RFC-0002 Phase F: one IR class registration a block reaches admission v10**
            // (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`). Sizing an IR program costs every node up to
            // seconds, so a further `ClassRegisteredTirV1` is dropped by name HERE — before any rent,
            // slot or fee is read or charged, exactly as the below-fence drop above — and the block
            // stands. The place is taken by the one handed to the gate (see there), whatever the gate
            // then decides; one dropped before the gate (unsigned, at a stale target, refused by the
            // rehearsal, over the slot cap) never took it. The fold refuses a second as its second lock.
            // **The second IR fence, likewise**: below `palw_tir_fence2` its moves (an IR step demand, an
            // answer or unit of that kind) are payloads an older build cannot decode and skips (A-2), so
            // they are dropped here, first, and charged nothing; the fold refuses them too.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_tir_fence2_v1(&object) && !self.palw_tir_fence2_at(point.daa_score)
            {
                info!(
                    "Block {block}: a second-IR-fence object was dropped by name below palw_tir_fence2, and the block stands (RFC-0002 Phase F)"
                );
                continue;
            }
            // **Pipeline-claim data availability, likewise** (spec 17 §17.14): below `palw_improvement_v1` its
            // moves (a pipeline step demand, an answer or unit of that kind) are payloads an older build cannot
            // decode and skips (A-2), so they are dropped here, first, and charged nothing; the fold refuses
            // them too.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_pipeline_da_v1(&object)
                && !self.palw_pipeline_da_at(point.daa_score)
            {
                info!(
                    "Block {block}: a pipeline-claim DA object was dropped by name below palw_improvement_v1, and the block stands (RFC-0004 Phase F)"
                );
                continue;
            }
            // **RFC-0006, likewise**: below `palw_tir_shard_v1` the layer-sharded panels' objects (a plan, a part, a shard
            // possession proof) and the `TirStepRun` unit and its answer are payloads an older build cannot decode and
            // skips (A-2), so they are dropped here, first, and charged nothing; the fold refuses them too.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_tir_shard_v1(&object) && !self.palw_tir_shard_at(point.daa_score) {
                info!(
                    "Block {block}: a layer-sharded-panel object was dropped by name below palw_tir_shard_v1, and the block stands (RFC-0006)"
                );
                continue;
            }
            if self.palw_model_immutable_v1_at(point.daa_score)
                && let Some(name) = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_model_definition_update_v1(&object)
            {
                info!("Block {block}: {name} refused by immutable model registration (ADR-0175); the block stands");
                continue;
            }
            // **RFC-0004 §6.3: an IR close carrying a composite artifact's sub-root openings** (the
            // parameter carriage's appended tag 2) is dropped by name below `palw_improvement_v1` —
            // an older build cannot decode the carriage and skips the object (A-2) — first, and
            // charged nothing. The fold refuses it too, as the second lock.
            if kaspa_consensus_core::palw_improve_composite_v1::palw_object_carries_composite_opening_v1(&object)
                && !self.palw_improvement_at(point.daa_score)
            {
                info!(
                    "Block {block}: an IR close with composite openings was dropped by name below palw_improvement_v1, and the block stands (RFC-0004)"
                );
                continue;
            }
            // **Decision 7a: a composite opening only for a class a governed line admitted as a composite
            // candidate**, with its recorded reference — dropped first and charged nothing otherwise; the
            // fold refuses it too.
            if let Err(why) = kaspa_consensus_core::palw_improve_composite_v1::palw_composite_openings_admitted_v1(&object, |class| {
                folded.improvement_composite_class(class)
            }) {
                info!("Block {block}: an IR close with composite openings was dropped, and the block stands: {why} (RFC-0004)");
                continue;
            }
            // **RFC-0004: an improvement object (tags 70–82) is dropped by name** — below
            // `palw_improvement_v1` for the IR objects' reason above (an older build skips it
            // undecoded), and above it until the object's admission lands — first, and charged
            // nothing. The fold refuses it too, as the second lock.
            if let Some(name) = kaspa_consensus_core::palw_state_v2::palw_improvement_object_name_v1(&object) {
                let active = self.palw_improvement_at(point.daa_score);
                if !active {
                    info!("Block {block}: {name} was dropped by name below palw_improvement_v1, and the block stands (RFC-0004)");
                    continue;
                }
                if !kaspa_consensus_core::palw_state_v2::palw_improvement_object_landed_v1(&object) {
                    info!("Block {block}: {name} was dropped by name: its admission has not landed, and the block stands (RFC-0004)");
                    continue;
                }
            }
            // **RFC-0004 A5: a composite candidate's admission sizes an IR program too**, so it shares the
            // block's one place at admission sizing with the IR registrations: a second is dropped by name.
            // **RFC-0001 §2.10: an adapter class listing sizes one too** (the same admission, over the class's
            // composite), so below `palw_adapter_class_v1` it is dropped by name first and past it it shares the place.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_adapter_class_v1(&object)
                && !self.palw_adapter_class_at(point.daa_score)
            {
                info!(
                    "Block {block}: an adapter class listing was dropped by name below palw_adapter_class_v1, and the block stands (RFC-0001 §2.10)"
                );
                continue;
            }
            let sizes_a_composite = matches!(
                &object,
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::CandidateSubmitted { payload, .. }
                    if payload.artifact.composite().is_some()
            ) || kaspa_consensus_core::palw_state_v2::palw_object_is_adapter_class_v1(&object);
            if tir_registration_gated && sizes_a_composite {
                info!(
                    "Block {block}: a second composite candidate or IR registration was dropped by name, and the block stands: one a \
                     block reaches admission sizing (RFC-0004 A5, PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1)"
                );
                continue;
            }
            // **RFC-0003 decision 22, likewise**: below `palw_held_close_chunks_v1` a held leaf challenge is a payload
            // an older build cannot decode and skips (A-2), so it is dropped here, first, and charged nothing; the
            // fold refuses it too.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_held_close_chunks_v1(&object)
                && !self.palw_held_close_chunks_at(point.daa_score)
            {
                info!(
                    "Block {block}: a held leaf challenge was dropped by name below palw_held_close_chunks_v1, and the block stands (RFC-0003)"
                );
                continue;
            }
            // **RFC-0007 Part I, likewise**: below `palw_verification_vertex_v1` a verification vertex or an equivocation is a
            // payload an older build cannot decode and skips (A-2), so it is dropped here, first, and charged nothing; the
            // fold refuses it too.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_vertex_v1(&object) && !self.palw_vertex_at(point.daa_score) {
                info!(
                    "Block {block}: a verification vertex move was dropped by name below palw_verification_vertex_v1, and the block stands (RFC-0007)"
                );
                continue;
            }
            // **RFC-0008 v2, likewise**: below `palw_exec_payload_v2` a work-session root declaration is a payload an older build
            // cannot decode and skips (A-2), so it is dropped here, first, and charged nothing; the fold refuses it too.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_exec_v2(&object) && !self.palw_exec_v2_at(point.daa_score) {
                info!(
                    "Block {block}: a work-session root declaration was dropped by name below palw_exec_payload_v2, and the block stands (RFC-0008 v2)"
                );
                continue;
            }
            // **RFC-0007 Part IV.1, likewise**: a trap commitment or reveal below `palw_audit_mesh_v1` is a payload an older build
            // cannot decode and skips; dropped by name, first, and charged nothing.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_mesh_v1(&object) && !self.palw_audit_mesh_at(point.daa_score) {
                info!(
                    "Block {block}: an audit mesh move was dropped by name below palw_audit_mesh_v1, and the block stands (RFC-0007)"
                );
                continue;
            }
            // **RFC-0010, likewise**: below `palw_permissionless_panel_v1` a certified Panel epoch output is a payload an older build
            // cannot decode and skips; dropped by name, first, and charged nothing. Past it the fold verifies the proof against the
            // branch and drops one that does not verify — a bad carried proof never invalidates a block.
            if kaspa_consensus_core::palw_state_v2::palw_object_is_panel_v3_v1(&object)
                && !state_params.panel_v3_rule_at(point.daa_score)
            {
                info!(
                    "Block {block}: a permissionless-Panel certified output was dropped by name below palw_permissionless_panel_v1, and the block stands (RFC-0010)"
                );
                continue;
            }
            // **Lane PA, G-3 (`palw_audit_1004_v1`)**: past the fence a generative registration is sized like an IR one and takes the
            // same one place a block.
            if tir_registration_gated
                && (matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredTirV1 { .. })
                    || (state_params.audit_1004_active_at(point.daa_score)
                        && matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredGenV1 { .. })))
            {
                info!(
                    "Block {block}: a second IR class registration was dropped by name, and the block stands: one a block reaches \
                     admission v10 (PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1, RFC-0002 Phase F)"
                );
                continue;
            }
            // **ADR-0075 SA-1: a chunk group's opener pays for the SLOT it takes.**
            //
            // A group holds one of `PALW_OBJECT_CHUNK_MAX_GROUPS` rows in the state root for up to
            // `PALW_OBJECT_CHUNK_TTL_DAA` — about five and a half days at a 120 s cadence — and
            // that row is denied to every honest drill just as completely by a group declaring two
            // parts as by one declaring eight. Pricing the DECLARED `count` therefore priced the
            // wrong resource, and priced it in the griefer's favour: his optimum was `count = 2`,
            // eight of which held the whole table for a quarter of what the rule was reasoned
            // about and for LESS per row than ADR-0075 D14's own 3- and 4-part drills pay. The
            // rent is the slot, flat, whatever the opener says it will send; chunks that merely
            // EXTEND an open group pay nothing, because the row is already paid for and charging
            // them would fall on the carrier that completes.
            //
            // Refused BEFORE the group is opened, so the junk never reaches the state root.
            if rent_armed
                && let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectChunk { group, .. } = &object
                && folded.pending_chunk_group(group).is_none()
            {
                let owed = kaspa_consensus_core::palw_state_v2::palw_object_chunk_group_rent_v1();
                if carrier_fee < owed {
                    info!(
                        "Block {block}: an ObjectChunk opening group {group} was dropped, and the block stands: its carrier paid \
                         {carrier_fee} sompi and one of the {} pending-chunk slots rents for {owed} (ADR-0075 SA-1)",
                        kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_GROUPS
                    );
                    continue;
                }
            }
            // **ADR-0080 design A: a declared close buys one adjudication, and pays for it before
            // the group opens.**
            //
            // The court lane's `palw_certification_min_fee_v1`, on the object that carries the
            // count the work is a function of — see `palw_court_close_min_fee_v1` for why it is the
            // DECLARATION and not the completing chunk: chunks arrive in any order, so "the last
            // one" is the declarer's choice of `index` and pricing on it is a discount of up to
            // `max_close_chunks`. Refused BEFORE the row is written, exactly as the slot rent above
            // is, so an underpaid declaration never reaches the state root and never commits any
            // validator to the assembly it would have bought.
            //
            // Dropped rather than block-invalidating, for SA-2's own reason: admission on the
            // lifecycle band is stateless, so an underpaid carrier relays and mines freely and it
            // would be honest miners producing the invalid blocks. Behind the same
            // `palw_certification_rent` fence as the two rents it copies — `None` on every shipped
            // preset, and dormant this whole block is skipped.
            // **ADR-0088 Decision 11's rent, actually collected** (audit 2026-09-19).
            //
            // `palw_object_rent_ceiling_v1` prices a line founding, a proposal and an evaluation at
            // `PALW_MODEL_OBJECT_RENT_SOMPI_V1`, and the coinbase burns that much — but it burns
            // `min(rent, fee)` (utxo_validation.rs), which is a CEILING on the burn and never a
            // floor on the fee. A carrier paying one sompi burned one sompi, so the anti-spam price
            // of the permissionless registry was not collected from anyone. The two objects above
            // are refused by name when their carrier underpays; these three were not, and they are
            // the three that fill a bounded table.
            //
            // Refused BEFORE the row is written, exactly as the two above are, and dropped with the
            // block standing for the reason audit M-01 gives: admission on the lifecycle band is
            // stateless, so invalidating would put honest miners on the losing side of somebody
            // else's underpayment.
            //
            // On the same dormant fence as the other two: `palw_certification_rent` is `None` on
            // every shipped preset, so this changes no block until an operator arms it.
            if rent_armed
                && matches!(
                    &object,
                    kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ModelLineFounded { .. }
                        | kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ModelProposalPosted { .. }
                        | kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ModelEvaluationPosted { .. }
                )
            {
                let owed = kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v1(&object);
                if carrier_fee < owed {
                    info!(
                        "Block {block}: a model-registry object was dropped, and the block stands: its carrier paid \
                         {carrier_fee} sompi and a row in a bounded registry table rents for {owed} (ADR-0088 Decision 11)"
                    );
                    continue;
                }
            }
            if rent_armed
                && let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::CourtCloseDeclared { session_id, count, .. } =
                    &object
            {
                let owed = kaspa_consensus_core::palw_state_v2::palw_court_close_min_fee_v1(*count as u64);
                if carrier_fee < owed {
                    info!(
                        "Block {block}: a CourtCloseDeclared for session {session_id} was dropped, and the block stands: its \
                         carrier paid {carrier_fee} sompi and adjudicating a close of {count} chunks rents for {owed} \
                         (ADR-0080 design A)"
                    );
                    continue;
                }
            }
            // ADR-0075 Decision 9: a block grades at most `PALW_CERTIFICATION_MAX_PER_BLOCK`
            // family drills. Counted before grading, in transaction order, so every node drops the
            // same object and the grader is never run for a drill the block may not carry.
            // A chunk that completes its group applies the FamilyCertified it carried, so it
            // counts against the same cap (ADR-0075 Decision 14).
            //
            // **A chunk the transition will refuse on its face completes nothing, and must not
            // spend the cap.** This asked only `count == 1` for a group that does not exist yet,
            // so `ObjectChunk { group: anything, index: 5, count: 1 }` — sixty bytes, refused by
            // the transition as `ChunkIndexOutOfRange` — was charged as a grading. Two of them per
            // block exhaust `PALW_CERTIFICATION_MAX_PER_BLOCK` and every genuine `FamilyCertified`
            // in that block is dropped, for two ordinary fees.
            //
            // **The index rule alone bought nothing, and that was the whole defect.** `index: 0,
            // count: 1` was always available at the identical sixty bytes, is refused by the
            // transition just as fast (`ChunkGroupHashMismatch`), and starves the identical cap —
            // so a version of this that asked only `index < count` deleted a strictly dominated
            // variant and left the attack. What the cap is spent on is the `FamilyCertified` the
            // completing chunk carries, so "completes" has to mean what the TRANSITION means by
            // it: every part present, the assembled bytes hashing to the declared group id, and
            // the object they decode to being a `FamilyCertified`. That is
            // [`Self::palw_chunk_completes_a_certification_v1`], and asking it here is the same
            // agreement-with-the-transition this whole rehearsal exists to keep.
            //
            // **Behind ADR-0075 D14's fence, `None` on every shipped preset.** It decides which
            // objects a block accepts and therefore its state root, so an upgraded node and an
            // un-upgraded one must not disagree about it in silence — see
            // [`Self::palw_chunk_cap_charge_at`]. Dormant, every added conjunct is `true` and the
            // predicate is byte-identical to the one before the fence existed.
            let cap_charged_on_grading = self.palw_chunk_cap_charge_at(point.daa_score);
            // Armed: the fault-vector count of the certification this chunk would assemble, read
            // ONCE here rather than decoded again for the work charge below. `None` — and dormant,
            // always `None` — means the payload was never touched.
            let mut chunk_vectors: Option<usize> = None;
            let completes_a_group = match &object {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectChunk { group, index, count, bytes } => {
                    let pending = folded.pending_chunk_group(group);
                    // The cheap structural test decides almost every chunk, and dormant it is the
                    // WHOLE test — byte-identical to the accounting before the fence existed.
                    let structurally_completes = match pending {
                        Some(p) => p.count == *count && !p.parts.contains_key(index) && p.parts.len() + 1 == *count as usize,
                        None => *count == 1,
                    };
                    if !structurally_completes {
                        false
                    } else if !cap_charged_on_grading {
                        true
                    } else if *index >= *count {
                        // `ChunkIndexOutOfRange`: the transition refuses it on its face.
                        false
                    } else {
                        // Armed, and only here does anything touch the payload: the transition's
                        // own completion test, and the grading work the object it assembles to
                        // will cost.
                        chunk_vectors = Self::palw_chunk_completes_a_certification_v1(
                            pending.map(|p| &p.parts),
                            *index,
                            *count,
                            bytes,
                            group,
                            self.palw_tir_at(point.daa_score),
                        );
                        chunk_vectors.is_some()
                    }
                }
                _ => false,
            };
            // **G14 lane D: a chunk group that carries a kernel route object is not a certification.** The grading cap and its work
            // budget are the family court's; the kernel route's own bounds are its fold's (and the carrier's fee).
            let chunk_inner = self.palw_kernel_chunk_inner(&folded, &object, point.daa_score);
            // Origin proof work has its own direct/chunk budget. A dormant inner still answers None (A-2).
            let origin_witness = match &object {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ExecOriginRecordedV1 { witness } => Some(witness),
                _ => match &chunk_inner {
                    Some(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ExecOriginRecordedV1 { witness }) => {
                        Some(witness)
                    }
                    _ => None,
                },
            };
            if let Some(witness) = origin_witness {
                if let Err(why) = folded.exec_v2_origin_witness_metadata_admissible_v1(state_params, point.daa_score, witness) {
                    info!("Block {block}: an EXEC origin proof was dropped before header decoding, and the block stands: {why}");
                    continue;
                }
                if let Err(why) = exec_origin_budget.charge() {
                    info!("Block {block}: an EXEC origin proof was dropped before header verification, and the block stands: {why}");
                    continue;
                }
            }
            let completes_a_group = completes_a_group
                && !chunk_inner.as_ref().is_some_and(|inner| {
                    (state_params.exec_v2_active_at(point.daa_score)
                        && matches!(inner, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ExecOriginRecordedV1 { .. }))
                        || kaspa_consensus_core::palw_state_v2::palw_object_is_kernel_route_v1(inner)
                        || kaspa_consensus_core::palw_state_v2::palw_object_is_onboarding_v1(inner)
                        || kaspa_consensus_core::palw_state_v2::palw_object_is_provider_court_v1(inner)
                        || kaspa_consensus_core::palw_state_v2::palw_object_is_legacy_held_da_v2(inner)
                });

            let is_certification =
                matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::FamilyCertified { .. })
                    || completes_a_group;
            // **What the block's cap counts is what the court will actually re-execute.**
            //
            // The predicate above answers "does this chunk finish its group", which is not the same
            // question. `ObjectChunk { group: <fresh>, index: 5, count: 1 }` finishes a group by
            // that reading and the transition then refuses it as `ChunkIndexOutOfRange`; so does
            // `{ index: 0, count: 1, bytes: [1 byte] }`, which assembles into bytes no object
            // decodes from. Sixty bytes either way, and two of them per block exhausted
            // `PALW_CERTIFICATION_MAX_PER_BLOCK` and dropped every genuine `FamilyCertified` the
            // block carried — a block-cheap way to keep an honest class weightless.
            //
            // `palw_v2_graded_vector_count` is exactly the "will the court grade this" predicate:
            // it assembles, decodes, and answers `None` for everything the transition will refuse
            // before `apply_object(FamilyCertified)`. Counting on IT rather than on "completes a
            // group" charges the cap for court work and for nothing else — and it subsumes the
            // narrower `index < count` reading of the same hole, so the two compose: with both
            // rules in force this one already refuses everything that one does.
            //
            // Behind the SAME fence as the two rents (`palw_certification_rent`), because it
            // decides which objects a block accepts and therefore its state root — an upgraded and
            // an un-upgraded node must never answer that differently in silence. Dormant, the
            // count is skipped entirely and the predicate below is the one that shipped.
            let graded_vectors =
                rent_armed.then(|| Self::palw_v2_graded_vector_count(&folded, &object, self.palw_tir_at(point.daa_score))).flatten();
            // Which of the two fences answers depends on which is armed, and they compose in one
            // direction: `palw_chunk_completes_a_certification_v1` (D14) also demands the assembled
            // bytes hash to the declared group id, so anything it calls a completion
            // `palw_v2_graded_vector_count` calls one too. The slot gate is therefore never tighter
            // than the work gate, and no object is charged a grading it was not also priced for.
            let charges_a_grading_slot = if rent_armed { graded_vectors.is_some() } else { is_certification };
            if charges_a_grading_slot {
                // **ADR-0075 SA-2: grading is priced before it is performed.**
                //
                // The court re-executes one recorded fault vector per vector the object carries,
                // up to `PALW_CERTIFICATION_MAX_VECTORS`, on every node, whether the drill passes
                // or is refused — and the vector count is a field the submitter writes, tied to
                // nothing about how much the object cost to carry. So the count is priced here,
                // from the object alone, and an underpaid one is dropped BEFORE it is graded and
                // before it consumes a grading slot: charging it a slot would let a griefer starve
                // the honest drills for free, which is the same denial the cap exists to stop.
                //
                // **DROPPED, where the amendment says "a block that grades one paying less is
                // invalid".** The security property is the same — nothing underpaid is ever
                // graded, so the CPU a block costs every validator is bounded by fees its carriers
                // paid — and the difference is who dies for a stranger's transaction. Admission on
                // the 0x4b band is stateless (decode, version, may-ride), so an underpaid carrier
                // relays and mines freely; making the accepting block invalid is audit M-01's
                // shape exactly, where "one ~100-byte transaction, one ordinary fee, and the chain
                // stops" — and a template builder selects transactions by fee without ever
                // consulting this filter, so honest miners would be the ones producing the invalid
                // blocks. Dropping needs no mempool rule to be safe; invalidity would.
                if let Some(vectors) = graded_vectors {
                    let owed = kaspa_consensus_core::palw_state_v2::palw_certification_min_fee_v1(vectors);
                    if carrier_fee < owed {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped ungraded, and the block stands: its carrier paid \
                             {carrier_fee} sompi and grading {vectors} fault vectors rents for {owed} (ADR-0075 SA-2)"
                        );
                        continue;
                    }
                }
                // **And the SLOT itself is ADR-0075 D14's question, under D14's own fence.**
                // Lane C's rule above decides what may be charged; this decides WHEN. The two are
                // separate fences because they were found separately and each is sound alone: the
                // rent bounds what a block's grading may cost its carriers, and D14 bounds what a
                // slot may be spent on. Dormant, this is the accounting that shipped.
                if !cap_charged_on_grading {
                    // **Dormant — every shipped preset — this is the pre-fence accounting verbatim.**
                    if certifications_graded >= kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped, and the block stands: the block already carries \
                             {} (PALW_CERTIFICATION_MAX_PER_BLOCK)",
                            kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK
                        );
                        continue;
                    }
                    certifications_graded += 1;
                } else {
                    // **Armed: a SLOT is what a certification the grader accepted has spent, and the
                    // grading WORK is charged in fault vectors before the grader runs.**
                    //
                    // Charging the slot up front was the defect, and the chunk rule closed only the
                    // side door: `matches!(object, FamilyCertified{..})` charged it too, before
                    // `palw_v2_validate_objects` or the transition ever ran. So an object the court
                    // refuses at its first line — `NoVectors`, decided before a single fault vector is
                    // graded — took a slot from a family that had earned one, and two of them per
                    // block dropped every genuine certification the block carried. Proven by
                    // experiment twice, with the fence armed, in both the direct and the chunked
                    // shape.
                    //
                    // Slots are therefore charged in the `Ok` arm of the apply below, to objects that
                    // WERE graded and accepted, which preserves Decision 9 exactly: at most
                    // `PALW_CERTIFICATION_MAX_PER_BLOCK` families enter the state per block. What an
                    // object costs before it is graded is the vectors it asks the grader to walk —
                    // `Vec::len`, free to read — against the block's fixed work budget, so a
                    // certification the court will refuse for free costs the block nothing.
                    //
                    // **Residual, stated rather than hidden.** The work budget is still first-come:
                    // an attacker who builds `PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK` vectors of
                    // well-formed-but-wrong evidence exhausts it and starves the block. That costs it
                    // two `PalwShapeProfileV3`s per vector — the first thing the grader compares — so
                    // the price of the attack rises from two sixty-byte carriers to tens of kilobytes
                    // of structurally plausible drill, and it is bounded by the block's own byte
                    // limit. Pricing it outright is a deposit, which is ADR-0075's chunk-deposit work
                    // and not this fence.
                    if certifications_graded >= kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped, and the block stands: the block already certified {} \
                             families (PALW_CERTIFICATION_MAX_PER_BLOCK)",
                            kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK
                        );
                        continue;
                    }
                    let vectors = match &object {
                        kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::FamilyCertified { evidence } => {
                            evidence.vector_count()
                        }
                        _ => chunk_vectors.unwrap_or(0),
                    };
                    if vectors_graded.saturating_add(vectors) > PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK {
                        info!(
                            "Block {block}: a FamilyCertified object was dropped, and the block stands: it would ask for {vectors} more \
                             fault vectors than the block's remaining grading budget ({} of {PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK} \
                             already spent)",
                            vectors_graded
                        );
                        continue;
                    }
                    vectors_graded += vectors;
                }
            }
            // **ADR-0080 design A: a block completes at most one declared close.**
            //
            // Counted here, in transaction order, against the state each accepted object leaves
            // behind — the same rehearsal every other per-block cap is counted against — and the
            // extra one is DROPPED with the block standing, mirroring
            // `PALW_CERTIFICATION_MAX_PER_BLOCK` exactly. Invalidating instead would be audit
            // M-01's shape: admission on the lifecycle band is stateless, so a second completing
            // carrier relays and mines freely, and honest miners would be the ones producing the
            // invalid blocks.
            //
            // Not fenced, because the objects it counts do not exist before this state version:
            // a chain that can carry a `CourtCloseChunk` at all is one that already re-minted for
            // `PALW_STATE_V2_VERSION` 18, so there is no un-upgraded node to disagree with.
            //
            // **ADR-0082 Decision 2 counts here too**, on the same slot: a dissection move is
            // court CPU the carrying block must spend (a fold checked, an artifact proven, a
            // binding verified), and giving it a second per-block budget would be two answers to
            // "how much court may one block ask for". Same drop-not-invalidate shape, same
            // constant, one counter.
            //
            // **Audit C-1: the slot is SPENT below, once acceptance has said the object is real.**
            // Both halves of the disjunction are state-dependent predicates over the folded state
            // — `palw_court_move_spends_the_slot_v1` replaced a bare variant match that let an
            // unauthenticated `CourtAttnRootClaimed` burn the network's only slot for the price of
            // one transaction fee — but a predicate cannot see a signature or the k-ary fence, and
            // `palw_v2_validate_objects` can. So the CAP is read here, before any work is asked
            // for, and the COUNTER moves after acceptance returns `Ok`: an object acceptance drops
            // did no court work (the drop path does none), and must not deny the block's real
            // close its slot. A close denied inside its assembly window is not a delay, it is the
            // conviction of the party that filed it.
            // **ADR-0042 Decision 10's queue, on the carrier lane** (audit M-10). A market move
            // writes fee legs into `pending_payouts`, which drains at
            // `PALW_V2_MAX_PAYOUTS_PER_BLOCK` and is capped at `PALW_V2_MAX_PENDING_PAYOUTS`. The
            // fold refuses the move that would breach the cap; the rehearsal drops it here so the
            // block stands. Read against `folded`, which is the state this block's earlier objects
            // have already moved — the same state the fold will see.
            {
                use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as MObj;
                let rows = match &object {
                    MObj::ModelBuy { .. } => 2usize,
                    MObj::ModelSell { .. } => 3usize,
                    MObj::ModelSeed { .. } => 0usize,
                    _ => 0usize,
                };
                if rows > 0 {
                    let held = folded.pending_payouts_iter().count();
                    let cap = kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PENDING_PAYOUTS;
                    let promised = model_payout_rows_promised.saturating_add(refund_rows_reserved);
                    if held.saturating_add(promised).saturating_add(rows) > cap {
                        info!(
                            "Block {block}: a model-market move was dropped, and the block stands: the payout queue holds {held} \
                             rows and this block has already promised {promised} more, against a cap of {cap} \
                             (PALW_V2_MAX_PENDING_PAYOUTS)"
                        );
                        continue;
                    }
                    if !exact_queue {
                        model_payout_rows_promised += rows;
                    }
                }
            }
            // **2026-09-24 DoS audit #12 (b): at most `PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1`
            // bought registrations a block** (past `palw_audit_2026_09_23`; genesis rows are never
            // here). Counted in transaction order like the two caps above and the extra one DROPPED
            // with the block standing, for the reason they give. The slot bounds the graph gate —
            // the one expensive thing a registration asks of every node — so it is charged BEFORE
            // the gate runs, and it stays charged when the gate refuses. The fold holds the same
            // number as its second lock (`ClassRegistrationsPerBlockExceeded`) and never sees more
            // than this admits: this counter is the ONLY thing between a fifth registration and a
            // disqualified block, because the rehearsal below folds each object on a fresh builder
            // whose own counter is always zero.
            //
            // **What a slot costs, and who can spend one** (review of #12). The first cut charged a
            // slot to any registration its registrant SIGNED, and everything after the signature
            // was free to fail with the slot kept. Two cheap ways to fill all four with objects
            // certain to be refused followed: a bond at the 0.04 MSK registration floor signing
            // registrations the fold refuses as `ClassRegistrationBurnUnaffordable` (so nothing is
            // ever burned), and a stranger with no bond at all relaying copies of registrations
            // already on chain — the signature binds no nonce and still verifies — which the fold
            // refuses as `DuplicateClass`. Either one censored every honest registration for four
            // carrier fees a block. So a slot is now charged only to a registration that
            //   * its registrant signed (`palw_v2_class_registration_is_signed`),
            //   * starts at the chain's target (`palw_v2_class_registration_starts_at_the_chains_target`,
            //     the gate's own O(1) first refusal, so a copy made stale by a retarget is free),
            //   * the fold's own `ClassRegistered` arm APPLIES on the state this block has reached —
            //     the burn affordable, the class not already live, the share grantable — asked by
            //     folding it (`palw_v2_apply_one_object_v1`), so the answer cannot drift from the
            //     fold's; the result is kept and reused below, so an accepted registration is folded
            //     once, as before; and
            //   * comes from a registrant none of whose registrations the gate has refused in this
            //     block.
            // What is left is a registrant that can afford the burn, signs a registration the fold
            // would take, and makes it fail the graph gate on purpose: one slot per block per such
            // bond, each bond holding 1 MSK plus the reservation it does not lose. That is a
            // capital price, not a burn — a refused object never reaches the fold, so no rule here
            // can charge it — and it is stated rather than hidden.
            let mut rehearsed_registration: Option<kaspa_consensus_core::palw_state_v2::PalwChainStateV2> = None;
            let bought_registration = if self.palw_audit_2026_09_23_at(point.daa_score) {
                kaspa_consensus_core::palw_state_v2::palw_class_registration_buyer_v1(&object)
            } else {
                None
            };
            if let Some(registrant) = bought_registration {
                if let Err(why) = self.palw_v2_class_registration_is_signed(&folded, &object) {
                    info!("Block {block}: a class registration was dropped, and the block stands: {why}");
                    continue;
                }
                if let Err(why) = self.palw_v2_class_registration_starts_at_the_chains_target(&folded, &object) {
                    info!("Block {block}: a class registration was dropped, and the block stands: {why}");
                    continue;
                }
                if class_registrants_refused.contains(&registrant) {
                    info!(
                        "Block {block}: a class registration was dropped, and the block stands: its registrant's registration was \
                         already refused by the admission gate in this block"
                    );
                    continue;
                }
                // **Lane PA, G-3 (`palw_audit_1004_v1`): the per-block cap is asked BEFORE the rehearsal fold** — a registration the
                // cap drops anyway is not folded first (each fold of a pipeline class is the expensive step).
                if state_params.audit_1004_active_at(point.daa_score)
                    && class_registrations_charged >= kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1
                {
                    info!(
                        "Block {block}: a class registration was dropped, and the block stands: the block already carries {} \
                         (PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1)",
                        kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1
                    );
                    continue;
                }
                if audit_active {
                    match kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_v1(
                        &folded,
                        state_params,
                        point,
                        &object,
                        self.palw_unavailable_abstains_at(point.daa_score),
                        self.palw_capability_bound_at(point.daa_score),
                        self.palw_uncertified_weightless_at(point.daa_score),
                        self.palw_da_court_at(point.daa_score),
                        &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
                            own_attempt_class,
                            history_custody_pledges: history_custody_pledges.clone(),
                            public_original_bindings: public_original_bindings.clone(),
                            model_classes_registered_in_block: model_classes_registered_in_block.clone(),
                            ..self.palw_transition_extras_for_objects(&folded, point, std::slice::from_ref(&object))
                        },
                    ) {
                        Ok(next) => rehearsed_registration = Some(next),
                        Err(why) => {
                            info!("Block {block}: a class registration was dropped, and the block stands: {why}");
                            continue;
                        }
                    }
                }
                if class_registrations_charged >= kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1 {
                    info!(
                        "Block {block}: a class registration was dropped, and the block stands: the block already carries {} \
                         (PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1)",
                        kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1
                    );
                    continue;
                }
                class_registrations_charged += 1;
            }
            // **ADR-0152 v3.1 addendum §4-bis.3: the heavy prompt budget, and what holding it costs.**
            // A `PromptNotAnchored { Whole }` (13) asks every node to recompute an anchor's whole
            // prompt root (13.6 ms at 2M); a block holds `PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1` of them
            // — one 2M check. Three rules keep a failing Whole from taking that slot for a fee (the
            // Phase 3 review):
            //   * its carrier pays the prompt's carriage as burned rent (`palw_object_rent_ceiling_v2`,
            //     dropped here when underpaid, as the other rents are);
            //   * it is charged only if it will REACH the recompute
            //     (`palw_offence_heavy_prompt_charge_v1`: decoded, the accused named, the target
            //     resolved, every cheap check of 13 passed) — junk that fails early costs nothing;
            //   * a claim is charged ONCE per block (the root is remembered), so a failing Whole on a
            //     claim never costs an honest one on the same claim the slot.
            // Charged before the gate computes and kept when the gate then refuses; the object that
            // would breach the budget is dropped and the block stands. The fold charges by the same
            // function as its second lock.
            //
            // **ADR-0160 F-B (the lane-verify review's heavy-slot finding): both readers here run
            // before the gate, so both read the batch fence.** Below it a kind-3 whose receipt is
            // `Windowed` is what `0e8ec984e` reads — bytes that do not decode — so it owes no prompt
            // rent and is charged nothing; the gate then refuses it by name. Read otherwise, an
            // upgraded node spent the block's one 2M slot on it and dropped an honest Whole behind it
            // that the base admitted.
            let batch_licence = state_params.capacity_batch_active_at(point.daa_score);
            if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence { kind, accused, evidence, .. } =
                &object
                && self.palw_offence_attribution_at(point.daa_score)
            {
                if rent_armed {
                    let owed = kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v3(
                        &object,
                        true,
                        batch_licence,
                        self.palw_legacy_public_filer_at(point.daa_score),
                    );
                    if carrier_fee < owed {
                        info!(
                            "Block {block}: a whole-prompt PromptNotAnchored was dropped, and the block stands: its carrier paid \
                             {carrier_fee} sompi and recomputing its prompt rents for {owed}"
                        );
                        continue;
                    }
                }
                if let Some(charge) = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_heavy_prompt_charge_at_v2(
                    &folded,
                    accused,
                    *kind,
                    evidence,
                    self.palw_identity_rules_v1(point.daa_score),
                    batch_licence,
                    self.palw_session_rule_at(point.daa_score),
                ) && !heavy_prompt_claims.contains(&charge.claim_id)
                {
                    let (heavy, budget) =
                        (charge.prompt_ids, kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1);
                    if heavy_prompt_ids_charged.saturating_add(heavy) > budget {
                        info!(
                            "Block {block}: a whole-prompt PromptNotAnchored ({heavy} ids) was dropped before it was computed, and \
                             the block stands: the block has already charged {heavy_prompt_ids_charged} of {budget} \
                             (PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1)"
                        );
                        continue;
                    }
                    heavy_prompt_ids_charged += heavy;
                    heavy_prompt_claims.insert(charge.claim_id);
                }
            }
            let spends_the_court_slot = kaspa_consensus_core::palw_state_v2::palw_court_close_completes_a_group_v1(&folded, &object)
                || kaspa_consensus_core::palw_state_v2::palw_court_move_spends_the_slot_v1(&folded, &object);
            if spends_the_court_slot && court_closes_completed >= kaspa_consensus_core::palw_state_v2::PALW_COURT_CLOSE_MAX_PER_BLOCK {
                info!(
                    "Block {block}: a court move that spends the block's adjudication slot was dropped, and the block \
                     stands: the block already spent {} (PALW_COURT_CLOSE_MAX_PER_BLOCK)",
                    kaspa_consensus_core::palw_state_v2::PALW_COURT_CLOSE_MAX_PER_BLOCK
                );
                continue;
            }
            // A completing owned terminal carries the same heavy-prompt budget as a direct offence.
            // Its rent was bought by the signed declaration; partial chunks never consume this slot.
            if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveEvidenceChunkV1 { part } = &object
                && let Some(row) = folded.terminal_pending_v1(&(part.claim, part.reserver))
            {
                let maximum = self.palw_court_params_v2.as_ref().map_or(0, |c| c.max_close_bytes());
                if let Ok(Some(evidence)) = row.preview(part, maximum) {
                    let d = &row.declaration;
                    if let Some(charge) =
                        kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_heavy_prompt_charge_at_v2_with_budget_v1(
                            &folded,
                            &d.accused,
                            d.kind,
                            &evidence,
                            self.palw_identity_rules_v1(point.daa_score),
                            batch_licence,
                            self.palw_session_rule_at(point.daa_score),
                            kaspa_consensus_core::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::ReservedTerminal {
                                court_maximum: maximum,
                            },
                        )
                    {
                        if charge.prompt_ids > u64::from(d.heavy_prompt_ids) {
                            continue;
                        }
                        if !heavy_prompt_claims.contains(&charge.claim_id) {
                            if heavy_prompt_ids_charged.saturating_add(charge.prompt_ids)
                                > kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1
                            {
                                continue;
                            }
                            heavy_prompt_ids_charged += charge.prompt_ids;
                            heavy_prompt_claims.insert(charge.claim_id);
                        }
                    }
                }
            }
            // RFC-0002 Phase F: the IR registration handed to the gate takes the block's one place at
            // admission v10. One the slot accounting above did not see (a network without the
            // 2026-09-23 audit, or the genesis registrant named) is first put to the gate's own two
            // O(1) refusals, so a copy nobody signed never takes the place.
            if matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredTirV1 { .. })
                || (state_params.audit_1004_active_at(point.daa_score)
                    && matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredGenV1 { .. }))
            {
                if bought_registration.is_none() {
                    if let Err(why) = self.palw_v2_class_registration_starts_at_the_chains_target(&folded, &object) {
                        info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                        continue;
                    }
                    if let Err(why) = self.palw_v2_class_registration_is_signed(&folded, &object) {
                        info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                        continue;
                    }
                }
                tir_registration_gated = true;
            }
            // A composite candidate takes the place only once its signature holds, so a copy nobody
            // signed never takes it.
            if sizes_a_composite {
                if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::CandidateSubmitted { payload, submitter, signature } =
                    &object
                    && let Err(why) = self.palw_improvement_candidate_is_signed(&folded, payload, submitter, signature)
                {
                    info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                    continue;
                }
                if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::AdapterClassListed { payload, lister, signature } =
                    &object
                    && let Err(why) = self.palw_adapter_listing_is_signed(&folded, payload, lister, signature)
                {
                    info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                    continue;
                }
                tir_registration_gated = true;
            }
            match self.palw_v2_validate_objects(&folded, state_params, point, std::slice::from_ref(&object)) {
                Ok(()) => {
                    // **AC-SLOT below the fence: the slot is charged on the acceptance check** (the
                    // pre-audit behavior). Past the fence it is charged only after the move applies.
                    if !audit_active && spends_the_court_slot {
                        court_closes_completed += 1;
                    }
                    // A bought registration was already folded before its slot was charged (above);
                    // that result is this one, computed once.
                    let applied = if let Some(next) = rehearsed_registration.take() {
                        Ok(next)
                    } else if audit_active {
                        kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_v1(
                            &folded,
                            state_params,
                            point,
                            &object,
                            self.palw_unavailable_abstains_at(point.daa_score),
                            self.palw_capability_bound_at(point.daa_score),
                            self.palw_uncertified_weightless_at(point.daa_score),
                            self.palw_da_court_at(point.daa_score),
                            &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
                                own_attempt_class,
                                history_custody_pledges: history_custody_pledges.clone(),
                                public_original_bindings: public_original_bindings.clone(),
                                model_classes_registered_in_block: model_classes_registered_in_block.clone(),
                                ..self.palw_transition_extras_for_objects(&folded, point, std::slice::from_ref(&object))
                            },
                        )
                    } else {
                        // The pre-audit path: rehearse the object through a whole-block transition
                        // at a synthetic point one blue score along. Byte-identical to before the
                        // audit fence existed.
                        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v2_with_extras(
                            &folded,
                            state_params,
                            &rehearsal,
                            std::slice::from_ref(&object),
                            None,
                            self.palw_unavailable_abstains_at(point.daa_score),
                            self.palw_capability_bound_at(point.daa_score),
                            self.palw_uncertified_weightless_at(point.daa_score),
                            self.palw_da_court_at(point.daa_score),
                            &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
                                history_custody_pledges: history_custody_pledges.clone(),
                                public_original_bindings: public_original_bindings.clone(),
                                model_classes_registered_in_block: model_classes_registered_in_block.clone(),
                                ..self.palw_transition_extras_for_objects(&folded, point, std::slice::from_ref(&object))
                            },
                        )
                        .map(|(next, _)| next)
                    };
                    match applied {
                        Ok(next) => {
                            // **AC-SLOT past the fence: the slot is spent only by a move that
                            // ACTUALLY APPLIES.** A move the fold refuses is dropped below and
                            // leaves the slot for the next.
                            if audit_active && spends_the_court_slot {
                                court_closes_completed += 1;
                            }
                            if !audit_active {
                                rehearsal.blue_score = rehearsal.blue_score.saturating_add(1);
                            }
                            if completes_a_group {
                                // The certification a chunk group carried is applied inside the
                                // chunk's own arm, so the kinds tally below never names it; say so
                                // here, in the same words a directly carried one gets, so an
                                // operator reading every validator's log for the same verdict
                                // (ADR-0075 §7) sees the family land.
                                info!(
                                    "Block {block}: PALW lifecycle carried 1× FamilyCertified (assembled from its chunks; {} attempt-lane and {} free-prompt-lane families are chain-certified now)",
                                    next.chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt)
                                        .len(),
                                    next.chain_certified_families(
                                        kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt
                                    )
                                    .len()
                                );
                            }
                            // **Armed: the slot is spent HERE, by a certification the grader
                            // accepted.** Dormant it was already charged before the grader ran,
                            // which is the accounting this fence replaces.
                            if cap_charged_on_grading && is_certification {
                                certifications_graded += 1;
                            }
                            folded = next;
                            // **ADR-0067: keep the declaration the chain just accepted.** The
                            // state retains the class's economic facts and drops the carriage;
                            // a node that will SERVE the class needs the graph, so it is indexed
                            // here — the accept path IBD replays, which is what makes a syncing
                            // node's index arrive with its chain.
                            if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                                class_id,
                                artifact_root,
                                admission: Some(carriage),
                                ..
                            } = &object
                            {
                                match borsh::to_vec(carriage.as_ref()) {
                                    Ok(bytes) => {
                                        let record = crate::model::stores::palw_class_carriage::PalwClassCarriageRecord {
                                            registered_daa: point.daa_score,
                                            artifact_root: *artifact_root,
                                            carriage: bytes,
                                        };
                                        if let Err(err) = self.palw_class_carriage_store.write().insert(*class_id, record) {
                                            warn!("[palw-class-carriage] could not index class {class_id}: {err}");
                                        }
                                    }
                                    Err(err) => {
                                        warn!("[palw-class-carriage] class {class_id}'s carriage does not re-serialize: {err}")
                                    }
                                }
                            }
                            if self.palw_model_immutable_v1_at(point.daa_score) {
                                use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
                                if let Obj::ClassRegistered { class_id, .. }
                                | Obj::ClassRegisteredTirV1 { class_id, .. }
                                | Obj::ClassRegisteredGenV1 { class_id, .. } = &object
                                    && state.class(class_id).is_none()
                                {
                                    model_classes_registered_in_block.insert(*class_id);
                                }
                            }
                            accepted.push(object);
                        }
                        Err(why) => {
                            info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                        }
                    }
                }
                Err(why) => {
                    // Review of #12: a charged registration the gate refused bars its registrant
                    // from another slot in this block (see the charging site).
                    if let Some(registrant) = bought_registration {
                        class_registrants_refused.insert(registrant);
                    }
                    info!("Block {block}: a PALW lifecycle object was dropped, and the block stands: {why}");
                }
            }
        }
        Self::palw_v2_settle_carrier_refund(
            &mut refund_owed,
            accepted.len(),
            folded.pending_payouts_iter().count().saturating_add(model_payout_rows_promised),
            &mut refund_rows_reserved,
            &mut refunds,
            block,
        );
        // **What this block actually carried.** A dropped object says so; an ACCEPTED one said
        // nothing at all, so "the chain is not carrying courts" and "the chain is carrying courts
        // and something later discards them" looked identical from every log on every node. On the
        // testnet-11 drill that ambiguity cost a full investigation: 176 `CourtOpened` submitted
        // with zero mempool refusals and zero drops, and no way to tell whether they were reaching
        // blocks. One line per block that carried anything, kinds only.
        if !accepted.is_empty() {
            let mut kinds: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
            for object in &accepted {
                *kinds.entry(palw_object_kind_name(object)).or_default() += 1;
            }
            info!(
                "Block {block}: PALW lifecycle carried {}",
                kinds.iter().map(|(k, n)| format!("{n}× {k}")).collect::<Vec<_>>().join(", ")
            );
        }
        // **The rehearsal's end state, handed back rather than recomputed.** ADR-0064 has to ask
        // "what does the bond registry look like once this block's own objects are applied", and
        // the only honest answer is the one this loop just built. Recomputing it beside this
        // function would be a second fold obliged to agree with the first, which is the shape of
        // defect ADR-0064 exists to remove. Only bond records are read out of it: every field of
        // one is a function of the objects and `daa_score`, never of the synthetic blue score the
        // rehearsal advances.
        (accepted, folded, refunds)
    }

    /// The extras every production fold and every acceptance rehearsal carry (ADR-0088 D11,
    /// ADR-0089 D9). The action list is the EVM step's and is added by the caller that has it.
    /// The fold's extras for the block `point` names — every fence resolved at its DAA score, and
    /// (ADR-0132 Upgrade C) its own `bits`, read from its header where the store holds one.
    pub(super) fn palw_transition_extras_for(
        &self,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
        let daa_score = point.daa_score;
        // SW-8's step 4c, read once: `(anchor_delay, reach)` where the block anchors a panel — and, past
        // lane F2's fence, the same reading's draw points (the header and mergeset are read once).
        let sw8_anchor = self.palw_sw8_chain_anchor_v1(point);
        let sw8 = sw8_anchor.as_ref().map(|(anchor_delay, _, anchor)| (*anchor_delay, anchor.reach));
        let sw8_draw = match &sw8_anchor {
            Some((_, block_daa, anchor)) if self.palw_floor_refusal_retry_at(daa_score) => {
                self.palw_sw8_draw_points_of_anchor_v1(*block_daa, anchor)
            }
            _ => None,
        };
        kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
            // ADR-0165: the reds of the accepting block's mergeset belong to the chain walk's own fold (a block's GHOSTDAG
            // data, set where it is folded — `extras.merged_reds`); every other reading of the fences (a rehearsal, the
            // pre-object base) merges no work, so nothing in it is red.
            merged_reds: Default::default(),
            audit_1004_draw_seed_source: None,
            model_lines_active: self.palw_model_lines_active_at(daa_score),
            model_immutable_active: self.palw_model_immutable_v1_at(daa_score),
            class_semantics_active: self.palw_class_semantics_v1.is_some_and(|f| f.is_active(daa_score)),
            staged_public_court: self
                .palw_staged_public_court_v1
                .filter(|f| f.is_active(daa_score))
                .map(|_| kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1()),
            history_custody_pledges: Default::default(),
            public_original_bindings: Default::default(),
            model_classes_registered_in_block: Default::default(),
            model_benefits_active: self.palw_model_benefits_active_at(daa_score),
            evm_market_active: self.palw_model_evm_active_at(daa_score),
            // Written explicitly like the fences below it: this one decides what every move pays.
            model_leg_v2_active: self.palw_model_leg_v2_active_at(daa_score),
            // And this one decides which payment opens a market (ADR-0120).
            model_seed_v2_active: self.palw_model_seed_v2_active_at(daa_score),
            // And this one opens every line's market on the virtual reserve (ADR-0162).
            model_virtual_v1: self.palw_model_virtual_v1_from_at(daa_score),
            // Written explicitly, never left to `..Default::default()`: an unwritten default inside
            // a struct the fold reads is the one fault no golden can see, and this field decides
            // whether a producer's collateral is destroyed.
            court_responder_coverage_active: self.palw_court_responder_coverage_at(daa_score),
            // Written explicitly for the same reason as the line above: this one decides how long a
            // producer owes its retained trace, and an unwritten default would silently give every
            // claim the producer's own number back.
            fp_da_pins_active: self.palw_fp_da_pins_at(daa_score),
            // Written explicitly for the same reason again: this one decides whether a boundary
            // grows a class's share on accepted blocks or on Final work (ADR-0107).
            share_growth_final_active: self.palw_share_growth_final_at(daa_score),
            // ADR-0123: the fold re-runs admission on every merged attempt, so it must release what
            // admission released for this block. Explicit for the reason the lines above give.
            epoch_budget_release_active: self.palw_epoch_budget_release_at(daa_score),
            // ADR-0124: these two decide who a `Final` claim pays and what a seat reserves and can
            // lose. Explicit for the reason every line above gives.
            panel_economy_active: self.palw_panel_economy_active_at(daa_score),
            work_priced_reward_active: self.palw_work_priced_reward_active_at(daa_score),
            // ADR-0130: what a seat put on duty in this block reserves — the duty row stores it, so
            // this is read once per binding and never at release. Explicit for the same reason.
            panel_reward_multiple_permille: self.palw_panel_reward_multiple_permille_at(daa_score),
            // ADR-0125: the lane's span, where it is open. The permits a block accepted are the
            // chain walk's to add — it is the only caller holding the verdicts.
            round_lane: self.palw_execution_lane_at(daa_score).map(|lane| {
                kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1 {
                    schedule_span_daa: lane.schedule_span_daa_at(daa_score),
                    execution_quantum: if self.palw_execution_quanta_at(daa_score) {
                        kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXECUTION_QUANTUM_V1
                    } else {
                        0
                    },
                    span_open_round: self
                        .headers_store
                        .get_timestamp(point.block)
                        .ok()
                        .map(|ts| kaspa_consensus_core::palw_execution_lane_v1::palw_execution_round_v1(ts, self.genesis.timestamp))
                        .unwrap_or(0),
                }
            }),
            // **ADR-0151's economic-safety fold.** The three values the pricing needs that the
            // transition cannot see: the cadence (a `Params` quantity — the transition holds only
            // `PalwStateParamsV2`), the declared worth of one stolen round, and the maturity the
            // network serves (testnet-12: 120 DAA, user decision 2026-09-25). `None` where the
            // fence is dormant, and then every path it touches is byte-identical.
            economic_safety: self.palw_economic_safety_at(daa_score).then_some({
                kaspa_consensus_core::palw_state_v2::PalwEconomicSafetyFoldV1 {
                    target_time_per_block_ms: self.palw_target_time_per_block_ms,
                    permit_value_sompi: kaspa_consensus_core::palw_economic_safety_v1::PALW_T12_PERMIT_FEE_CEILING_SOMPI,
                    maturity_daa: self.palw_exec_quantum_maturity_daa,
                }
            }),
            round_permit_uses: Vec::new(),
            round_fee_incomes: Vec::new(),
            // RFC-0008 v2: the slice carriers the covered set holds (empty until the closure carriage fills it).
            exec_v2_covered: Vec::new(),
            exec_v2_origin_headers: Default::default(),
            exec_v2_anchor: None,
            // ADR-0126 Decision 3: the carve this block's own attempt escrows at — the block that
            // carried it is this one, and the block that pays it is its selected-chain child, always
            // later, so the lower score is this one's. Explicit for the reason every line above
            // gives: it decides how much of the subsidy a claim holds.
            escrow_carve: self.palw_escrow_for_carrier_at(point.block, daa_score, daa_score),
            model_registry: self.palw_model_registry_fold_at(daa_score),
            // ADR-0132 Upgrade C: the rate, the shares, the ceiling and this block's `bits`. The
            // same reading at every site that folds this block, because it is derived from the
            // point and the header store alone.
            economic_payout: self.palw_economic_payout_fold_for(point),
            // ADR-0137 (shadow): the work target's fold input — the rate, the clamp and every
            // class's work — on every ConsensusV2 network, so every node prints the same `W`.
            work_target: self.palw_work_target_fold_for(point),
            work_target_active: self.palw_work_target_at(daa_score),
            artifact_root_ownership_active: self.palw_artifact_root_ownership_at(daa_score),
            operator_id_unique_active: self.palw_operator_id_unique_at(daa_score),
            // **ADR-0145: the HEIGHT, not this block's answer.** Every other fence here is resolved
            // at `daa_score` because it decides something about THIS block. This one decides the
            // unit a CLAIM's work is carried in, and a claim is priced at the block that finalizes
            // it, re-priced at the block that retires it, and re-derived by every consistency check
            // in between — so each of those sites compares the height against the claim's own
            // `accepted_daa` instead. Resolving it here would price one claim under two bases and
            // the node would refuse its own tip. `None` on every shipped preset.
            canonical_work_daa: self.palw_canonical_work_daa,
            // **ADR-0147: the HEIGHT.** The block-level rules ask it at this block
            // (`admission_independence_at`); the claim-level one — the outsider seat and its veto on
            // the licence — compares it against the CLAIM's own `accepted_daa`, as the draw does.
            admission_independence_daa: self.palw_admission_independence_daa(),
            // ADR-0145 §5/§6's height; the fold asks it at each block (`fp_derived_work_at`).
            fp_derived_work_daa: self
                .palw_fp_derived_work
                .filter(|fence| *fence != kaspa_consensus_core::config::params::ForkActivation::never())
                .map(|fence| fence.daa_score()),
            single_lottery_active: self.palw_single_lottery_at(daa_score),
            verification_v2_active: self.palw_verification_v2_at(daa_score),
            verification_s3_active: self.palw_verification_s3_at(daa_score),
            verification_s2_active: self.palw_verification_s2_at(daa_score),
            readiness_v2_active: self.palw_readiness_v2_at(daa_score),
            evm_actions: Vec::new(),
            // P-B1: the chain walk is the only caller holding the filter's verdicts, and adds them
            // itself, exactly as it adds the EVM's actions above.
            carrier_market_refunds: Vec::new(),
            // ADR-0093 Decision 8: which form of move 1 opens a phase. Written explicitly for the
            // reason the lines above give — an unwritten default here would refuse, or admit, a
            // responder's whole defense by omission.
            attn_anchored_root_active: self.palw_attn_anchored_root_at(daa_score),
            audit_2026_09_11_active: self.palw_audit_2026_09_11_at(daa_score),
            audit_2026_09_11_deep_active: self.palw_audit_2026_09_11_deep_at(daa_score),
            audit_2026_09_23_active: self.palw_audit_2026_09_23_at(daa_score),
            settled_anchor_depth: self.palw_settled_anchor_depth_at(daa_score),
            // ADR-0152 v2 F2: which route a false `Valid` takes at this block — the V1 kind refused
            // and `PanelFalseValidV2` consumed through the adjudicator the object gate ran. Written
            // explicitly for the reason every line above gives: an unwritten default here would fold
            // the V2 kind as dormant after the gate admitted it, and the V1 kind by the old rule
            // after the gate refused it.
            offence_attribution_active: self.palw_offence_attribution_at(daa_score),
            legacy_held_da_v2_active: self.palw_legacy_held_da_v2_at(daa_score),
            // ADR-0152-adjacent (Activation Pool): R1, R2 and the pool's terms at this block. Written
            // explicitly for the reason every line above gives: an unwritten default here would
            // reclaim a listing by the old rule on a network that has armed the new one.
            activation_pool: self.palw_activation_pool_at(daa_score),
            // ADR-0152 X7 / N9 (M3): the peer's P2-7 constant, and nothing else.
            seat_da_answer_landed: kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1,
            // ADR-0100: the one-move court's ladder rides to the fold when the court is armed —
            // the SAME ladder the acceptance arm adjudicates at, so both derive one verdict.
            // Written explicitly for the reason the two lines above give.
            shard_court_ladder: if self.palw_shard_court_at(daa_score) {
                self.palw_court_params_v2.as_ref().map(|court| self.palw_court_step_ladder_at(daa_score, court))
            } else {
                None
            },
            // ADR-0100 Decision 4: one shard's seats and quorum ride to the fold when per-shard
            // licensing is armed — the panel parameters the draw and the acceptance layer use.
            shard_licensing: if self.palw_shard_licensing_at(daa_score) {
                self.palw_panel_params_v2.as_ref().map(|panel| {
                    kaspa_consensus_core::palw_shard_licensing_v1::PalwShardLicensingParamsV1 {
                        seats_per_shard: panel.seat_count(),
                        quorum_per_shard: panel.quorum(),
                    }
                })
            } else {
                None
            },
            // ADR-0103: the held regime's ladder rides to the fold when it is armed — the SAME leaf
            // cap the acceptance arms open against. Written explicitly for the reason every field
            // above gives: this one decides whether a bisection is a legal move at all.
            held_context_ladder: if self.palw_held_context_at(daa_score) {
                self.palw_court_params_v2.as_ref().map(|court| self.palw_court_step_ladder_at(daa_score, court))
            } else {
                None
            },
            // ADR-0118 Decision 4: the network's genesis prompt-ids form, which a held DA demand for
            // a prompt tile is judged against. Written explicitly for the reason every field above
            // gives: on a network minted flat a tile of a non-held claim has no answer.
            prompt_ids_merkle: self.palw_prompt_ids_form_at(daa_score)
                == kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            // ADR-0144 §9: the HEIGHT, not this block's yes/no. Written explicitly so an unwritten
            // default cannot silently leave the lock ledger off on a network that has armed it.
            objective_offence_daa: self.palw_objective_offence_daa(),
            terminal_evidence_max_bytes: self
                .palw_legacy_public_filer_at(daa_score)
                .then(|| self.palw_court_params_v2.as_ref().map(|c| c.max_close_bytes()))
                .flatten(),
            // ADR-0133 §7: the HEIGHT, so the profile a row is derived with is a function of the
            // block that derived it and a resync folds what the live chain folded.
            seat_gate_possession_daa: self.palw_seat_gate_possession_daa(),
            // Only the acceptance rehearsal sets it, from the block's header
            // (`palw_v2_accepted_objects`); the fold takes it from the block's work.
            own_attempt_class: None,
            // ADR-0152 v3.1 J-1: set by the chain walk for the block's own attempt, never here.
            own_job_anchor: kaspa_hashes::Hash64::default(),
            // **ADR-0152 SW-8 / DL-1 (M4): step 4c's anchor delay**, on a block that may anchor a
            // panel past `palw_rcore_plus`. Written explicitly for the reason every line above
            // gives: an unwritten default here would leave a refused claim holding its reservation
            // for the whole bind window.
            sw8_anchor_delay: sw8.map(|(anchor_delay, _)| anchor_delay),
            // **Lane A: the highest slot this block anchors** — below lane A its own DAA score; past
            // it the latest operator attempt it is or merges. Written explicitly for the same reason:
            // a default here would void, at a block that merges a displaced operator attempt, claims
            // whose slot falls after that attempt and which the block never anchored.
            sw8_anchor_reach: sw8.map(|(_, reach)| reach),
            // **Lane F2: the block's draw points**, where it anchors and the floor-refusal retry is in force
            // at it. Written explicitly for the reason every line above gives: a default here would void,
            // in its anchor block, a claim no seed could have seated.
            sw8_draw,
            model_court_window_active: self.palw_model_court_window_active_at(daa_score),
            class_court_windows: std::collections::BTreeMap::new(),
            // RFC-0010: the permissionless Panel's draw inputs, resolved at the pre-entropy checkpoint (None while the fence is off).
            panel_v3: self.palw_panel_v3_inputs_for(point),
            // G14 lane D: the kernel route, where its (never armable) fence is in force. Explicit, like every fence above: a default
            // here would fold nothing at a block that must.
            kernel_route: self.palw_kernel_route_extras_at(daa_score),
        }
    }

    pub(super) fn palw_transition_extras_for_objects(
        &self,
        _state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2],
    ) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
        let mut extras = self.palw_transition_extras_for(point);
        extras.model_court_window_active = self.palw_model_court_window_active_at(point.daa_score);
        extras.class_court_windows = self.palw_class_court_windows_for_objects(objects, point.daa_score);
        extras
    }

    pub(super) fn palw_v2_objects_of_block(
        &self,
        acceptance: &AcceptanceData,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        block: BlockHash,
        block_daa: u64,
        carrier_fees: &std::collections::HashMap<TransactionId, u64>,
    ) -> Vec<PalwCarriedObjectV1> {
        let Some(freeprompt) = self.palw_freeprompt_params_v3.as_ref() else {
            return Vec::new();
        };
        let txs = self.accepted_txs_from_acceptance_data(acceptance);
        let history_armed = self.palw_staged_public_court_v1.is_some_and(|fence| fence.is_active(block_daa));
        let mut history_carriers = std::collections::HashMap::new();
        let mut original_bindings = std::collections::HashMap::new();
        let mut history_views = Vec::new();
        if history_armed {
            // Every accepted transaction keeps its place. An HFP1 carrier is replaced by its
            // original-payload view; any other carrier (a plain FP claim of a class with no
            // required family, an evaluation or tensor claim, an external producer's claim)
            // rides unchanged. Whether a claim NEEDS the envelope is the fold's question
            // (`prepare_required_bundle_v1` refuses a required-family claim without its pledge),
            // never a silent drop here after the fee was paid.
            history_views.reserve(txs.len());
            for tx in txs.iter() {
                if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT
                    || !tx.payload.starts_with(&kaspa_consensus_core::palw_history_custody_v1::PALW_HISTORY_FP_ENVELOPE_MAGIC_V1[..])
                {
                    history_views.push(tx.clone());
                    continue;
                }
                match kaspa_consensus_core::palw_history_custody_v1::PalwHistoryFpEnvelopeV1::decode_v1(&tx.payload) {
                    Ok(envelope) => {
                        history_carriers.insert(tx.id(), envelope.pledge);
                        original_bindings.insert(tx.id(), envelope.original_binding);
                        // Transient decoder view deliberately retains the outer
                        // cached carrier ID. It is never validated as a real TX.
                        let mut view = tx.clone();
                        view.payload = envelope.original_payload;
                        history_views.push(view);
                    }
                    // A malformed envelope carries no admissible original: its fee is the outer
                    // transaction's and it produces no object (the walk cannot decode it).
                    Err(why) => info!("[palw-history] carrier {} has no admissible original custody envelope: {why}", tx.id()),
                }
            }
        }
        let fp_txs = if history_armed { history_views.as_slice() } else { txs.as_slice() };
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        // **The ladder this network froze, read off the bundle and never typed** (ADR-0082
        // Decision 1). `max_step_leaf_count` is a bundle field — inside `palw_ruleset_id_v2`, so
        // it cannot move on a running chain — and it is DAA-free: `palw_court_params_at_v2` only
        // ever changes the dissection arity, never the ladder, so reading it here rather than
        // through the fence keeps this walk a pure function of the accepted set and the ruleset.
        //
        // A node with no bundle has no ladder to apply and falls back to the structural top, which
        // is what the arming-free entry uses; the walk is then exactly as permissive as it was.
        let ladder = self
            .palw_v2_bundle
            .as_ref()
            .map(|bundle| bundle.court.max_step_leaf_count())
            .unwrap_or(kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
        let extraction = kaspa_consensus_core::palw_fp_objects_v3::palw_fp_objects_from_accepted_txs_by_class_v1(
            fp_txs,
            network_domain,
            freeprompt,
            kaspa_consensus_core::BlockHash::default(),
            // ADR-0077 Decision 16 at the ACCEPTING block's DAA — candidate-scoped, like every
            // rule on this lane — and ADR-0081 Decision 3's form, which `validate_palw_v2` keeps
            // genesis-only, so the height it is read at cannot matter. This used to pass a literal
            // `false`, which on a genesis that arms `PanelDa` would have skipped every private
            // commitment the door had admitted.
            self.palw_panel_da_at(block_daa),
            // **ADR-0119 Decision 5: each commitment at its CLASS's bounds**, off the parent state:
            // a class under the held regime recorded its own ladder when it registered, and every
            // other class meets the network's ladder, exactly as before.
            |class_id| kaspa_consensus_core::palw_fp_objects_v3::PalwFpClassCapsV1 {
                step_ladder: state.class_step_ladder_v1(class_id, ladder),
                held: state.class_is_held_v1(class_id),
                // **ADR-0145 §5: past the fence the walk prices the run itself** (the 2026-09-19
                // reward audit's F2). Three states, at the ACCEPTING block's DAA like every rule
                // on this lane: below the fence the declared leaves ride as they always have;
                // past it the class's published graph is what counts them; past it with no graph
                // published the commitment is skipped and the carrier says so, which is the
                // operator's cue to carry one `ClassLaneCertified` for the class.
                derived_work: if self.palw_fp_derived_work_at(block_daa) {
                    match state.fp_work_profile_of(class_id) {
                        Some(profile) => kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Derived(profile),
                        None => kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Unpublished,
                    }
                } else {
                    kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Declared
                },
                // RFC-0001 §A.3 / RFC-0003 §I.3: whether the class commits its logits in Q24.
                logits_q24: state.class_commits_q24_logits_v1(class_id),
                // RFC-0001 §2.6 stage 2 and §2.9, at the ACCEPTING block's DAA.
                prefix_state_armed: self.palw_fp_prefix_state.is_some_and(|fence| fence.is_active(block_daa)),
                // RFC-0001 §2.6 stage 2b, at the ACCEPTING block's DAA.
                prefix_inherit_armed: self.palw_fp_prefix_inherit.is_some_and(|fence| fence.is_active(block_daa)),
                // RFC-0001 stage 2b: whether the class's leaves are inheritance-safe (refused by name otherwise).
                prefix_inherit_class_safe: state.class_prefix_inherit_safe_v1(class_id).is_ok(),
                // ADR-0096 Decision 8 and RFC-0001 §2.5, at the ACCEPTING block's DAA.
                constraint_armed: self.palw_fp_decode_constraint.is_some_and(|fence| fence.is_active(block_daa)),
                constraint_v2_armed: self.palw_fp_constraint_v2.is_some_and(|fence| fence.is_active(block_daa)),
                tokenizer: kaspa_consensus_core::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1::of(
                    self.palw_fp_tokenizer_match.is_some_and(|fence| fence.is_active(block_daa)),
                    state.class_tokenizer_listing_v1(class_id),
                ),
            },
            // **ADR-0044 Decision 9's two advertised caps, at the same block's DAA** (mainnet audit
            // 2026-09-06, L-2). The bundle's `max_prompt_tokens` and `max_decode_tokens` are inside
            // every node's `palw_ruleset_id_v2` and were read by nothing; past this fence the walk
            // reads them off the bundle it already holds. `None` on every shipped preset — both
            // live chains have accepted jobs above them since genesis.
            self.palw_fp_ruleset_caps.is_some_and(|fence| fence.is_active(block_daa)),
            // ADR-0103 Decision 4: under the held regime no PublicDa carrier's ids ride past one
            // standard transaction.
            self.palw_held_context_at(block_daa),
            self.palw_prompt_ids_form_at(block_daa),
            // RFC-0001 §A.4 at the ACCEPTING block: V3 jobs below `Params::palw_fp_decode_rules`,
            // V4 jobs from it. A V3 claim accepted before the fence is already state and keeps
            // the V3 verifier; this decides only what a new commitment may be.
            self.palw_fp_decode_rules_at(block_daa),
            // Who authored the commitment. Unverified, a 0x4a transaction from any stranger created
            // a claim bound to any bond outpoint it named — the genesis premine bond among them.
            Self::verify_mldsa87_with_context_bool,
        );
        let mut extraction = extraction;
        for (carrier, reason) in &extraction.skipped {
            info!("[palw-fp] carrier {carrier} produced no object: {reason}");
        }
        // **RFC-0004 A6: the evaluation claims — the free-prompt walk's twin for version-9 payloads**,
        // which that walk skipped above as undecodable (their bytes follow the FP payload's with the
        // claim's tail). Past `palw_improvement_v1` at the ACCEPTING block only; below it the walk does
        // not run, so a build that carries the fence and one that does not fold the same objects from
        // every block below it. The arguments are the free-prompt walk's, at the same bounds: each claim
        // meets its class's ladder and the held regime's rules, the ruleset's caps, the network's form
        // and this processor's verifier. `Declared`: an evaluation claim's leaves are the chain's count
        // of its derived context, which the fold holds, not the FP lane's shape profile.
        let evaluation = if self.palw_improvement_at(block_daa) {
            kaspa_consensus_core::palw_improve_eval_v1::palw_fp_eval_objects_from_accepted_txs_v1(
                fp_txs,
                network_domain,
                freeprompt,
                self.palw_panel_da_at(block_daa),
                |class_id| kaspa_consensus_core::palw_fp_objects_v3::PalwFpClassCapsV1 {
                    step_ladder: state.class_step_ladder_v1(class_id, ladder),
                    held: state.class_is_held_v1(class_id),
                    derived_work: kaspa_consensus_core::palw_fp_objects_v3::PalwFpDerivedWorkCapV1::Declared,
                    // The class's logit unit, as the FP walk asks it (RFC-0001 §A.3): an evaluation claim's
                    // stand-in job meets the same decode-control offer as any V4 job on its class.
                    logits_q24: state.class_commits_q24_logits_v1(class_id),
                    // An evaluation claim is not a prefix-state claim; its class's tokenizer listing binds it as any job's.
                    prefix_state_armed: false,
                    prefix_inherit_armed: false,
                    prefix_inherit_class_safe: false,
                    constraint_armed: false,
                    constraint_v2_armed: false,
                    tokenizer: kaspa_consensus_core::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1::of(
                        self.palw_fp_tokenizer_match.is_some_and(|fence| fence.is_active(block_daa)),
                        state.class_tokenizer_listing_v1(class_id),
                    ),
                },
                self.palw_fp_ruleset_caps.is_some_and(|fence| fence.is_active(block_daa)),
                self.palw_held_context_at(block_daa),
                self.palw_prompt_ids_form_at(block_daa),
                self.palw_fp_decode_rules_at(block_daa),
                Self::verify_mldsa87_with_context_bool,
            )
        } else {
            Default::default()
        };
        for (carrier, reason) in &evaluation.skipped {
            info!("[palw-eval] carrier {carrier} produced no object: {reason}");
        }
        // **RFC-0003 §I.4.4: the tensor claims (FP job version 10), past both fences.** The lane's walk
        // above skipped every version-10 payload as not stateless-admissible; this is its twin for them,
        // at each commitment's CLASS's ladder, under the network's domain and arming, and its objects are
        // appended after the lane's own — a fixed concatenation, as the lifecycle walk's below. Below
        // either fence it does not run: no tensor claim becomes an object.
        let tensor = self.palw_v2_gen_objects_of_txs(fp_txs, state, block_daa);
        for (carrier, reason) in &tensor.skipped {
            info!("[palw-fp] carrier {carrier} produced no tensor object: {reason}");
        }
        extraction.objects.extend(tensor.objects);
        let mut evaluation = evaluation;
        let mut history_pledges = std::collections::BTreeMap::<
            kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_history_custody_v1::PalwHistoryCustodyPledgeV1
        >::new();
        if history_armed {
            for objects in [&mut extraction.objects, &mut evaluation.objects] {
                objects.retain(|carried| {
                    // A plain carrier has no pledge to check; the fold alone decides whether its
                    // claim required one.
                    let Some(pledge) = history_carriers.get(&carried.carrier) else { return true };
                    if let Err(why) = pledge.authenticate_original_object_v1(&carried.object, Self::verify_mldsa87_with_context_bool) {
                        info!("[palw-history] original FP carrier {} pledge refused: {why}", carried.carrier);
                        return false;
                    }
                    if history_pledges.get(&pledge.claim).is_some_and(|old| old.pledge_id_v1() != pledge.pledge_id_v1()) {
                        info!("[palw-history] original FP carrier {} competes with an earlier same-claim pledge", carried.carrier);
                        return false;
                    }
                    history_pledges.entry(pledge.claim).or_insert_with(|| pledge.clone());
                    true
                });
            }
        }
        // P0-11: the claim-lifecycle objects. Without this walk no block could carry a
        // `PanelBound`, so every claim on a V2 network voided at `BindTimeout` and PALW weight —
        // the network's whole fork choice — was permanently zero.
        //
        // Appended after the free-prompt objects rather than interleaved by transaction order:
        // the two walks are independent pure functions of the same accepted set, so a fixed
        // concatenation is a deterministic order every node reproduces, and it keeps a change to
        // one walk from reordering the other's objects.
        let lifecycle = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(&txs);
        for (carrier, reason) in &lifecycle.skipped {
            info!("[palw-lifecycle] carrier {carrier} produced no object: {reason}");
        }
        // **A-2 uniformity: a kind whose owning fence is not in force at this block is the live build's undecodable payload here** —
        // that build's walk skips its carrier, so this one drops it HERE, before the acceptance walk reads it: no slot, rent, budget,
        // cap or refund is asked of it, nothing is folded, and the acceptance walk sees exactly the live build's object list
        // (`PalwLifecycleKindFencesV1`, the A2U review).
        let (lifecycle_objects, unread): (Vec<_>, Vec<_>) = lifecycle
            .objects
            .into_iter()
            .partition(|carried| self.palw_lifecycle_kind_fences.kind_in_force_at(&carried.object, block_daa));
        for carried in &unread {
            info!(
                "[palw-lifecycle] carrier {} produced no object: payload does not decode (A-2: its kind's owning fence is not in force \
                 at this block, so it is read as the live build reads it)",
                carried.carrier
            );
        }
        // The derived bindings go FIRST: a claim bound by this block may then be licensed by an
        // object the same block carries, which is the order a chain that is catching up needs.
        //
        // ADR-0075 SA-1/SA-2: each carried object keeps what its own carrier paid, resolved from
        // the map `calculate_utxo_state` filled while it had the composed UTXO view in hand. That
        // map holds EVERY accepted transaction, not a list of PALW bands, so a carrier this walk
        // can name is always in it: the two extractors below read `txs`, which is the accepted set
        // that loop just priced. The `unwrap_or(0)` is therefore unreachable rather than a
        // fail-closed branch — and it has to be, because for a priced object it is a drop, and for
        // certification a permanent halt on that network. Below the fence `PALW_RENT_UNPRICED`
        // makes the whole rule read as absent rather than as "everyone underpaid".
        let priced = self.palw_certification_rent_at(block_daa);
        let fee_of = |carrier: TransactionId| {
            if priced { carrier_fees.get(&carrier).copied().unwrap_or(0) } else { PALW_RENT_UNPRICED }
        };
        // **The 2026-09-23 Position route matrix, P-B1: a carrier-borne buy or seed names, here, who
        // it is paid back to if acceptance refuses it** — the one place the carrier transaction is
        // still in hand. Past `palw_audit_2026_09_23` only; below it no object carries a refund and
        // the whole P-B1 path is absent.
        let refunds_armed = self.palw_audit_2026_09_23_at(block_daa);
        // The object's kind first, so the carrier is looked up only for a paid market move (the
        // review: the lookup ran for every lifecycle object). `(refund, unrefundable)`: a buy or seed
        // whose carrier pays no P2PKH-ML-DSA-87 output is named, so its refusal is logged.
        let refund_of = |carried: &kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleCarrierV2| {
            use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as MObj;
            let paid = match &carried.object {
                MObj::ModelBuy { msk_in, .. } => *msk_in,
                MObj::ModelSeed { msk_seed, .. } => *msk_seed,
                // ADR-0152-adjacent (Activation Pool): a top-up the fold refuses (a missing or Frozen
                // class, under the least top-up) is paid back by the same route, against the same cap.
                MObj::ActivationPoolFunded { amount, .. } => *amount,
                // RFC-0004 (spec 17 §17.11.1): a deposit the fold refuses (a line not governed) is
                // paid back by the same route. (Only a bound carrier the walk extracted reaches here; below
                // the fence no carrier can bind an improvement sink, so none is ever refunded there.)
                MObj::ImprovementPoolFunded { payload } => payload.amount,
                _ => 0,
            };
            if !refunds_armed || paid == 0 {
                return (None, None);
            }
            match txs
                .iter()
                .find(|tx| tx.id() == carried.carrier)
                .and_then(|tx| kaspa_consensus_core::palw_lifecycle_objects_v2::palw_model_carrier_refund_v1(tx, &carried.object))
            {
                Some(refund) => (Some(refund), None),
                None => (None, Some(carried.carrier)),
            }
        };
        self.palw_v2_derived_panel_bindings(state, block, block_daa)
            .into_iter()
            // Nothing carried a derived binding, so nothing owes rent for it.
            .map(|object| PalwCarriedObjectV1 { object, carrier_fee: PALW_RENT_UNPRICED, history_pledge: None, original_binding: None, refund: None, unrefundable: None })
            .chain(extraction.objects.into_iter().chain(evaluation.objects).map(|carried| {
                let history_pledge = match &carried.object {
                    kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted { claim, .. } => history_pledges.get(claim).cloned(),
                    _ => None,
                };
                PalwCarriedObjectV1 {
                    carrier_fee: fee_of(carried.carrier), history_pledge, original_binding: original_bindings.get(&carried.carrier).cloned(),
                    object: carried.object, refund: None, unrefundable: None,
                }
            }))
            .chain(lifecycle_objects.into_iter().map(|carried| {
                let (refund, unrefundable) = refund_of(&carried);
                PalwCarriedObjectV1 { carrier_fee: fee_of(carried.carrier), history_pledge: None, original_binding: None, refund, unrefundable, object: carried.object }
            }))
            .collect()
    }

    /// **RFC-0009 stage D: the proof of one PALW collection against the header of `block`.**
    ///
    /// A header commits the root of the state as-of its SELECTED PARENT (the chain walk compares `header.palw_state_root` with the state
    /// it has reached BEFORE applying the block), so the state served is `palw_v2_state_at(selected_parent(block))` — the reconstruction
    /// the template and the validating walk both use — and it is refused unless it hashes to exactly what the header commits: a node never
    /// hands a client a proof that its own header contradicts. The header rides along; the client recomputes its hash against the block it
    /// pinned.
    ///
    /// **Bounded.** Walking the state back to an old block costs delta applications, and a flat proof of a table costs every row. A block
    /// more than [`PALW_STATE_PROOF_MAX_LAG_DAA`] behind the sink is refused (pin a newer checkpoint), and so is a collection whose rows
    /// would exceed [`PALW_STATE_PROOF_MAX_BYTES`] (a tree-shaped state commitment is the fix, and a consensus change).
    pub fn palw_state_proof_v1_impl(
        &self,
        block: BlockHash,
        collection: &[u8],
    ) -> Result<(kaspa_consensus_core::header::Header, kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1), String> {
        use kaspa_consensus_core::palw_state_proof_v1 as proof;
        if self.palw_state_params_v2.is_none() {
            return Err("this network has no PALW V2 state".to_string());
        }
        let header = self.headers_store.get_header(block).map_err(|_| format!("the node holds no header for block {block}"))?;
        if header.palw_state_root == kaspa_hashes::Hash64::default() {
            return Err("the header commits no PALW state root".to_string());
        }
        let sink_daa = self.virtual_stores.read().state.get().map(|v| v.daa_score).unwrap_or(0);
        if header.daa_score.saturating_add(PALW_STATE_PROOF_MAX_LAG_DAA) < sink_daa {
            return Err(format!(
                "block {block} is {} DAA behind the sink, more than the {PALW_STATE_PROOF_MAX_LAG_DAA} this node rebuilds state for: pin a newer checkpoint",
                sink_daa - header.daa_score
            ));
        }
        let parent = self
            .ghostdag_store
            .get_selected_parent(block)
            .map_err(|_| format!("the node holds no GHOSTDAG data for block {block}"))?;
        let (_at, state) = self
            .palw_v2_state_at(parent)
            .ok_or_else(|| format!("this node cannot establish the PALW state as-of {parent}, the selected parent of {block}"))?;
        // RFC-0009 L2: in the form the header commits (`palw_committed_root_v1`). Past `palw_fork_choice_commitment_v1` the proof's
        // preimage opens the ADR-0043 root inside the envelope; the client unwraps it with the fork-choice opening of `parent`.
        if self.palw_committed_root_v1(&state) != header.palw_state_root {
            return Err(format!(
                "the state this node holds as-of {parent} commits {}, and the header of {block} commits {}: nothing is served",
                self.palw_committed_root_v1(&state),
                header.palw_state_root
            ));
        }
        let fact = match collection {
            c if c == proof::PALW_PROOF_LABEL_BONDS => proof::prove_bonds_v1(&state),
            c if c == proof::PALW_PROOF_LABEL_CLASSES => proof::prove_classes_v1(&state),
            c if c == proof::PALW_PROOF_LABEL_CLAIMS => proof::prove_claims_v1(&state),
            other => return Err(format!("no such collection {:?}: bonds, classes or claims", String::from_utf8_lossy(other))),
        };
        let bytes: usize = fact.state.preimage.len() + fact.collection.rows.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>();
        if bytes > PALW_STATE_PROOF_MAX_BYTES {
            return Err(format!(
                "the {} table is {bytes} bytes, over the {PALW_STATE_PROOF_MAX_BYTES} a flat proof may carry: the state commitment is flat, and a tree-shaped one is a consensus change",
                String::from_utf8_lossy(collection)
            ));
        }
        Ok(((*header).clone(), fact))
    }
}
