//! Finish a signed receipt against one captured virtual generation. Every
//! trial is disposable; only normal block acceptance can consume the quantum.
use super::template::template_parent_fault_v1;
use super::*;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwOrderedReplayBudgetV1, PalwOrderedReplayLimitsV1, PalwOrderedReplayRulesV1,
};
use palw_candidate_v3::{
    NativeManifestCandidateInputV3, NativeManifestCandidateModeV3, NativeManifestTemplateInputsV3, PalwCandidateResultV3,
};

impl VirtualStateProcessor {
    pub fn finalize_palw_receipt_template_v3(
        &self,
        mut template: BlockTemplate,
        sign: &mut dyn FnMut(&mut Header),
    ) -> Result<BlockTemplate, RuleError> {
        let daa = template.block.header.daa_score;
        let Some(policy) = self.palw_manifest_ingress_v3.filter(|p| p.active_at(daa)) else {
            sign(&mut template.block.header);
            template.block.header.finalize();
            return Ok(template);
        };
        let unavailable = |why: &str| {
            RuleError::LocalNativeCapacityUnavailable(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                Self,
            >("receipt template convergence", why))
        };
        let local = PalwLocalTransitionFaultV1::into_rule_error_v1;
        // Warm the exact parent without holding the virtual read guard. The
        // following guard then keeps UTXOs, bonds and generation coherent.
        let warm = self.virtual_stores.read().state.get().map_err(|why| unavailable(&why.to_string()))?;
        let prepared = self.prepare_template_parent_v1(&warm).map_err(template_parent_fault_v1)?;
        let virtual_read = self.virtual_stores.read();
        let observed = virtual_read.state.get().map_err(|why| unavailable(&why.to_string()))?;
        if !Arc::ptr_eq(&warm, &observed) || template.selected_parent_hash != warm.ghostdag_data.selected_parent {
            return Err(unavailable("virtual generation changed; rebuild the template"));
        }
        self.check_prepared_template_parent_v1(&prepared).map_err(template_parent_fault_v1)?;
        let (parent_hash, parent) = prepared.state.as_ref().ok_or_else(|| unavailable("authenticated PALW parent absent"))?;
        if *parent_hash != template.selected_parent_hash {
            return Err(unavailable("template and PALW parent differ"));
        }
        let params = self.palw_state_params_v2.as_ref().ok_or_else(|| unavailable("PALW parameters absent"))?;
        let parent_multiset = self.utxo_multisets_store.get(*parent_hash).map_err(|why| unavailable(&why.to_string()))?;
        let reverse_virtual = warm.utxo_diff.as_reversed();
        let selected_parent_utxo_view = (&virtual_read.utxo_set).compose(&reverse_virtual);
        let bond_view = self.initial_active_bond_view();

        // The virtual's EXEC augmentation must be the one the literal own
        // coinbase asks the verifier to accept, not a later cached anchor.
        let mut ghostdag = self.ghostdag_manager.ghostdag(template.block.header.direct_parents());
        let exec_resolved = if self.palw_exec_v2_active_at(daa) {
            use kaspa_consensus_core::palw_exec_v2_anchor::{palw_exec_v2_anchor_split, palw_exec_v2_anchor_verify};
            let coinbase = template.block.transactions.first().ok_or_else(|| unavailable("coinbase absent"))?;
            match palw_exec_v2_anchor_split(&coinbase.payload).map_err(|why| unavailable(&why.to_string()))?.1 {
                Some(anchor) => {
                    let closure = self
                        .palw_exec_v2_closure(parent, *parent_hash, daa, &anchor.heads)
                        .map_err(|why| unavailable(&why.to_string()))?;
                    palw_exec_v2_anchor_verify(&anchor, &closure).map_err(|why| unavailable(&why.to_string()))?;
                    kaspa_consensus_core::blockhash::BlockHashes::make_mut(&mut ghostdag.mergeset_reds)
                        .extend(closure.members.iter().map(|member| member.hash));
                    Some(PalwExecV2ResolvedV1 { closure, anchor })
                }
                None => None,
            }
        } else {
            None
        };
        if ghostdag.selected_parent != *parent_hash
            || ghostdag.mergeset_blues.as_ref() != warm.ghostdag_data.mergeset_blues.as_ref()
            || ghostdag.mergeset_reds.as_ref() != warm.ghostdag_data.mergeset_reds.as_ref()
        {
            return Err(unavailable("literal EXEC anchor differs from the captured virtual mergeset"));
        }
        let mut remaining = PalwOrderedReplayLimitsV1 { passes: policy.replay_passes, candidate_visits: policy.replay_visits };
        let mut utxo_budget = manifest_requests_v3::NativeManifestReplayBudgetV3::new_v3(
            usize::try_from(policy.replay_passes).map_err(|_| unavailable("UTXO retry bound overflow"))?,
        );
        loop {
            self.check_shutdown_boundary().map_err(local)?;
            // Each hash is a real signed candidate, never ORIGIN. Re-signing
            // changes its point, but carries forward the remaining allowance.
            sign(&mut template.block.header);
            template.block.header.finalize();
            if template.block.header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3 {
                return Err(unavailable("receipt signer changed the lane"));
            }
            let header = Arc::new(template.block.header.clone());
            let inputs =
                NativeManifestTemplateInputsV3::from_virtual_v3(&warm, &header, &template.block.evm_payload).map_err(local)?;
            let point = PalwBlockContextV2 {
                block: header.hash,
                daa_score: daa,
                blue_score: header.blue_score,
                subsidy: self.coinbase_manager.calc_block_subsidy(daa),
            };
            let mut ctx = UtxoProcessingContext::new((&ghostdag).into(), parent_multiset.clone());
            ctx.palw_manifest_requests_v3 = self.preflight_manifest_candidate_v3(&ctx, parent, point, policy)?;
            if ctx.palw_manifest_requests_v3.is_none() {
                return Ok(template);
            }
            self.populate_palw_candidate_context_v3(&mut ctx, Some(parent), &header, &warm.mergeset_non_daa).map_err(local)?;
            ctx.positive_accepting_point = Some(point);
            if self.palw_tir_public_conviction_v1.is_some_and(|f| f.is_active(daa)) {
                ctx.palw_tfp1_history_accepting_point_v1 = Some(point);
            }
            if self.palw_lifecycle_kind_fences.batch_court_in_force_at(daa) {
                ctx.palw_batch_accepting_point_v1 = Some(point);
            }
            let mut tokenizer = if self.preflight_tokenizer_candidate_v1(&ctx, parent, daa)? {
                Some(
                    tokenizer_registration_v1::TokenizerRegistrationFeePrefixV1::begin(
                        self,
                        parent,
                        params,
                        point,
                        *self
                            .palw_lifecycle_kind_fences
                            .tokenizer_registration_v1_at(daa)
                            .ok_or_else(|| unavailable("tokenizer scanner without policy"))?,
                    )
                    .map_err(local)?,
                )
            } else {
                None
            };
            // Re-signing must not renew the initial UTXO traversal either.
            utxo_budget.charge_v3().map_err(RuleError::LocalNativeCapacityUnavailable)?;
            self.calculate_utxo_state_with_tokenizer_v1(
                &mut ctx,
                &selected_parent_utxo_view,
                &bond_view,
                daa,
                Some(parent),
                tokenizer.as_mut(),
            )
            .map_err(RuleError::LocalNativeCapacityUnavailable)?;
            let actions = self
                .take_assignment_actions_v2(&mut ctx, point, &selected_parent_utxo_view, Some(parent))
                .map_err(RuleError::LocalNativeCapacityUnavailable)?;
            let mut replay_budget = PalwOrderedReplayBudgetV1::new_v1(
                parent,
                params,
                point,
                PalwOrderedReplayRulesV1 {
                    unavailable_abstains: self.palw_unavailable_abstains_at(daa),
                    capability_bound: self.palw_capability_bound_at(daa),
                    uncertified_weightless: self.palw_uncertified_weightless_at(daa),
                    da_court: self.palw_da_court_at(daa),
                },
                remaining,
            )
            .map_err(|why| unavailable(&why.to_string()))?;
            let mut quote = |cost, retained, archive| policy.charge_v3(cost, retained, archive);
            let result = self
                .converge_manifest_candidate_v3(
                    header.hash,
                    &header,
                    &mut ctx,
                    &selected_parent_utxo_view,
                    &bond_view,
                    parent,
                    &parent_multiset,
                    actions,
                    tokenizer.as_mut(),
                    exec_resolved.as_ref(),
                    None,
                    &mut utxo_budget,
                    NativeManifestCandidateInputV3 {
                        mode: NativeManifestCandidateModeV3::ProjectTemplateFees,
                        template: Some(inputs),
                        projected_evm_actions: None,
                        maximum_archive_heap: policy.maximum_archive_heap,
                        maximum_archive_work: policy.maximum_archive_work,
                        replay_budget: &mut replay_budget,
                        quote: &mut quote,
                    },
                )
                .map_err(local)?;
            remaining = replay_budget.remaining_v1();
            match result {
                PalwCandidateResultV3::TemplateReady => {}
                PalwCandidateResultV3::Refused { stage, why } => return Err(unavailable(&format!("{stage}: {why}"))),
                _ => return Err(unavailable("template projection returned a commit-ready or incomplete result")),
            }
            let reward = self.coinbase_inputs_for_candidate_v3(
                &mut ctx,
                &bond_view,
                &header,
                &template.block.transactions,
                &warm.mergeset_non_daa,
                Some(parent),
            )?;
            let old_coinbase = template.block.transactions.first().ok_or_else(|| unavailable("coinbase absent"))?;
            let _decode_lease = self
                .coinbase_manager
                .reserve_coinbase_payload_decode_v2(&old_coinbase.payload)
                .map_err(|why| unavailable(&format!("coinbase decode: {why:?}")))?;
            let miner = self
                .coinbase_manager
                .deserialize_coinbase_payload(&old_coinbase.payload)
                .map_err(RuleError::BadCoinbasePayload)?
                .miner_data;
            let coinbase = self
                .coinbase_manager
                .expected_coinbase_transaction(
                    daa,
                    reward.own_subsidy,
                    miner,
                    &ctx.ghostdag_data,
                    &ctx.mergeset_rewards,
                    &warm.mergeset_non_daa,
                    reward.validator_reward_outputs.iter().chain(ctx.palw_v2_payout_outputs.iter()),
                    reward.carve.as_ref(),
                    reward.inclusion,
                    ctx.palw_v2_escrow_withheld,
                    &ctx.palw_v2_unentitled_blues,
                    true,
                    &ctx.palw_v2_merged_escrow_withheld,
                    &self.palw_round_blocks_of(&ctx.ghostdag_data),
                    &ctx.palw_v2_receipt_v4_payouts,
                )
                .map_err(|why| match why {
                    kaspa_consensus_core::errors::coinbase::CoinbaseError::LocalNativeCapacityUnavailable(why) => {
                        RuleError::LocalNativeCapacityUnavailable(why)
                    }
                    why => RuleError::BadCoinbasePayload(why),
                })?;
            template.block.transactions[0] = coinbase.tx;
            template.coinbase_has_red_reward = coinbase.has_red_reward;
            template.coinbase_miner_script_output_indices = coinbase.miner_script_output_indices;
            template = template.with_coinbase_allocation_v2(coinbase.allocation_v2);
            template.block.header.hash_merkle_root = calc_hash_merkle_root(template.block.transactions.iter());
            template.block.header.accepted_id_merkle_root =
                self.calc_accepted_id_merkle_root(ctx.accepted_tx_ids.iter().copied(), *parent_hash);
            template.block.header.utxo_commitment = ctx.multiset_hash.clone().finalize();
            self.recommit_manifest_template_evm_v3(
                &mut template.block.header,
                &mut ctx,
                &selected_parent_utxo_view,
                parent,
                &template.block.evm_payload,
            )
            .map_err(|why| match why {
                evm::EvmChainContextFaultV2::Local(why) => local(why),
                evm::EvmChainContextFaultV2::Invalid(why) => RuleError::EvmTemplateExecutionFailed(why),
            })?;
            // Fee-driven acceptance can invalidate a selected own transaction
            // or an EVM deposit. Refuse this template; do not silently drop it.
            let own_view = (&selected_parent_utxo_view).compose(&ctx.mergeset_diff);
            let fees = self.validate_transactions_in_parallel(&template.block.transactions, &own_view, daa, TxValidationFlags::Full);
            if fees.len() != template.block.transactions.len() - 1 {
                return Err(unavailable("own transactions changed validity after fee convergence"));
            }
            drop(fees);
            template.block.header.finalize();
            if template.block.header.hash == header.hash {
                self.check_prepared_template_parent_v1(&prepared).map_err(template_parent_fault_v1)?;
                return Ok(template);
            }
        }
    }
}
