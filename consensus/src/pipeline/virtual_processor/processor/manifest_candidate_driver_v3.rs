//! Bounded convergence of actual UTXO, EVM and PALW candidates. The explicit
//! Manifest owner is not supplied by any public preset. A caller must first
//! arm/capture the real UTXO pass using its bounded native preflight.

use super::*;
use palw_candidate_v3::{NativeManifestCandidateInputV3, NativeManifestCandidateModeV3, PalwCandidateResultV3};

impl VirtualStateProcessor {
    /// Count the actual physical source bodies before any request decode or
    /// UTXO trial. Refused bodies still count toward this private contract.
    pub(super) fn preflight_manifest_candidate_v3(
        &self,
        ctx: &UtxoProcessingContext<'_>,
        parent: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        point: kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        policy: kaspa_consensus_core::config::params::ManifestIngressFenceV3,
    ) -> Result<Option<manifest_requests_v3::NativeManifestRequestPassV3>, RuleError> {
        let exceeded = |why: &str| RuleError::PalwManifestIngressServiceExceeded(why.into());
        let local = |why: String| {
            RuleError::LocalNativeCapacityUnavailable(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                Self,
            >("Manifest physical sources", &why))
        };
        let ordered = self.acceptance_ordered_mergeset_without_selected_parent(&ctx.ghostdag_data, point.daa_score);
        let blocks = u64::try_from(ordered.len())
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| exceeded("source count overflow"))?;
        if blocks > policy.maximum_source_blocks {
            return Err(exceeded("source block bound"));
        }
        let mut transactions = 0u64;
        let mut requests = 0u64;
        let mut bytes = 0u64;
        for block in std::iter::once(ctx.selected_parent()).chain(ordered.iter().copied()) {
            let source_daa = self.headers_store.get_daa_score(block).map_err(|why| local(why.to_string()))?;
            let body = self.block_transactions_store.get(block).map_err(|why| local(why.to_string()))?;
            transactions = transactions
                .checked_add(u64::try_from(body.len()).map_err(|_| exceeded("transaction count overflow"))?)
                .filter(|count| *count <= policy.maximum_transactions)
                .ok_or_else(|| exceeded("physical transaction bound"))?;
            for tx in body.iter().filter(|tx| source_daa >= policy.activation.daa_score() && tx.payload.starts_with(b"PMR")) {
                requests = requests
                    .checked_add(1)
                    .filter(|count| *count <= policy.maximum_requests)
                    .ok_or_else(|| exceeded("physical request count bound"))?;
                bytes = bytes
                    .checked_add(u64::try_from(tx.payload.len()).map_err(|_| exceeded("payload length overflow"))?)
                    .filter(|count| *count <= policy.maximum_request_bytes)
                    .ok_or_else(|| exceeded("physical request byte bound"))?;
            }
        }
        if requests == 0 {
            return Ok(None);
        }
        manifest_requests_v3::NativeManifestRequestPassV3::new_v3(
            parent.state_root(),
            point,
            usize::try_from(requests).map_err(|_| exceeded("request allocation bound"))?,
            None,
        )
        .map(|pass| Some(pass.with_source_activation_v3(policy.activation.daa_score())))
        .map_err(RuleError::LocalNativeCapacityUnavailable)
    }

    /// Starts after actual UTXO collection; joint relay pricing happens here,
    /// before EVM or header verification. Every changed candidate is rebuilt
    /// from the immutable parent. Neither budget is renewed by the loop.
    /// Local failures abort this candidate; no block status or DB row is written.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn converge_manifest_candidate_v3<V: UtxoView + Sync>(
        &self,
        current: BlockHash,
        header: &Arc<Header>,
        ctx: &mut UtxoProcessingContext<'_>,
        selected_parent_utxo_view: &V,
        bond_view: &ActiveBondView,
        parent_state: &kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1,
        parent_multiset: &MuHash,
        mut actions: Vec<kaspa_consensus_core::palw_state_v2::PositiveAssignmentRawActionV2>,
        mut tokenizer_prefix: Option<&mut tokenizer_registration_v1::TokenizerRegistrationFeePrefixV1<'_>>,
        exec_resolved: Option<&PalwExecV2ResolvedV1>,
        evm_pipeline: Option<&crate::processes::evm::EvmPipeline>,
        utxo_budget: &mut manifest_requests_v3::NativeManifestReplayBudgetV3,
        input: NativeManifestCandidateInputV3<'_, '_>,
    ) -> Result<PalwCandidateResultV3, PalwLocalTransitionFaultV1> {
        let unavailable = |why| {
            kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>("Manifest candidate convergence", why)
        };
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: current,
            daa_score: header.daa_score,
            blue_score: header.blue_score,
            subsidy: self.coinbase_manager.calc_block_subsidy(header.daa_score),
        };
        if !matches!(input.mode, NativeManifestCandidateModeV3::ProjectFees | NativeManifestCandidateModeV3::ProjectTemplateFees)
            || ctx.positive_accepting_point != Some(point)
            || (input.mode == NativeManifestCandidateModeV3::ProjectTemplateFees) != input.template.is_some()
        {
            return Err(unavailable("driver requires fee projection at the actual accepting point").into());
        }
        ctx.palw_manifest_requests_v3
            .as_ref()
            .ok_or_else(|| unavailable("actual Manifest UTXO pass is absent"))?
            .check_scope_v3(parent_state.state_root(), Some(point))?;
        ctx.require_no_evm_stage_v3()?;
        let selected_parent = ctx.selected_parent();
        loop {
            self.check_shutdown_boundary()?;
            // Manifest and relay fees are projected together below. A separate
            // legacy relay preview would miss earlier Manifest effects and
            // reject already-debited inputs on the next disposable iteration.
            let projected_evm_actions = match self.project_manifest_evm_actions_v3(
                current,
                header,
                ctx,
                parent_state,
                input.template.map(|template| template.evm_payload),
            ) {
                Ok(actions) => actions,
                Err(evm::EvmChainContextFaultV2::Local(why)) => return Err(why),
                Err(evm::EvmChainContextFaultV2::Invalid(why)) => {
                    return Ok(PalwCandidateResultV3::Refused { stage: "EVM projection", why });
                }
            };
            let (projection, relay, fees_match_actual) = match self.prepare_palw_candidate_v3(
                current,
                header,
                ctx,
                selected_parent_utxo_view,
                bond_view,
                parent_state,
                actions,
                tokenizer_prefix.as_deref(),
                exec_resolved,
                Some(NativeManifestCandidateInputV3 {
                    mode: input.mode,
                    template: input.template,
                    projected_evm_actions: Some(projected_evm_actions),
                    maximum_archive_heap: input.maximum_archive_heap,
                    maximum_archive_work: input.maximum_archive_work,
                    replay_budget: &mut *input.replay_budget,
                    quote: &mut *input.quote,
                }),
            )? {
                PalwCandidateResultV3::Projected { fees, relay, fees_match_actual } => (fees, relay, fees_match_actual),
                refusal @ PalwCandidateResultV3::Refused { .. } => return Ok(refusal),
                PalwCandidateResultV3::Prepared(_) | PalwCandidateResultV3::TemplateReady => {
                    return Err(unavailable("projection unexpectedly finalized a candidate").into());
                }
            };
            let replay = self.replay_manifest_utxo_fees_v3(
                utxo_budget,
                ctx,
                selected_parent_utxo_view,
                bond_view,
                point,
                parent_state,
                parent_multiset.clone(),
                projection,
                Some(relay),
                tokenizer_prefix.as_deref_mut(),
            )?;
            actions = self.take_assignment_actions_v2(ctx, point, selected_parent_utxo_view, Some(parent_state))?;
            if !fees_match_actual || replay.acceptance_changed || replay.round_income_changed || replay.manifest_projection_changed {
                continue;
            }
            if input.mode == NativeManifestCandidateModeV3::ProjectTemplateFees {
                // The caller rebuilds commitments/rewards and re-signs if any
                // pre-pow field changes. This result never enters chain commit.
                return Ok(PalwCandidateResultV3::TemplateReady);
            }
            // Only converged UTXO acceptance may validate deposit locks and
            // the producer commitment. The projection above returned market
            // actions only and left the pipeline and commit-ready stage alone.
            match self.evm_chain_context_step(
                current,
                selected_parent,
                header,
                ctx,
                selected_parent_utxo_view,
                evm_pipeline,
                Some(parent_state),
            ) {
                Ok(()) => {}
                Err(evm::EvmChainContextFaultV2::Local(why)) => return Err(why),
                Err(evm::EvmChainContextFaultV2::Invalid(why)) => {
                    return Ok(PalwCandidateResultV3::Refused { stage: "EVM", why });
                }
            }
            let result = self.prepare_palw_candidate_v3(
                current,
                header,
                ctx,
                selected_parent_utxo_view,
                bond_view,
                parent_state,
                actions,
                tokenizer_prefix.as_deref(),
                exec_resolved,
                Some(NativeManifestCandidateInputV3 {
                    mode: NativeManifestCandidateModeV3::Strict,
                    template: None,
                    projected_evm_actions: None,
                    maximum_archive_heap: input.maximum_archive_heap,
                    maximum_archive_work: input.maximum_archive_work,
                    replay_budget: &mut *input.replay_budget,
                    quote: &mut *input.quote,
                }),
            )?;
            if matches!(result, PalwCandidateResultV3::Projected { .. } | PalwCandidateResultV3::TemplateReady) {
                return Err(unavailable("strict candidate returned only a fee projection").into());
            }
            return Ok(result);
        }
    }
}
