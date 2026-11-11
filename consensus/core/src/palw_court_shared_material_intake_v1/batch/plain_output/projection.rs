use super::PlainBatchOutputV2;
use crate::{
    Hash64,
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_service_v1::{ServiceCost, ServiceReservations, causal::CausalServicePacketV1},
    palw_court_shared_material_intake_v1::{
        MaterialIntakeScheduleV1,
        batch::{
            admission::{
                BatchAdmissionQuoteV1, BatchDeliveryV1, CompiledBatchAdmissionV1, NativeBatchDeliveryCostsV1,
                NativeBatchDeliveryShapeV1, compile_native_batch_plain_tokenizer_v2, plain_tokenizer::PlainTokenizerBatchBudgetV2,
            },
            native_ranges::{NumericalBatchProjectionV1, numerical_batch_projection_v1},
            plain_tokenizer::PlainBatchTokenizerV2,
        },
    },
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1},
    palw_positive_contract_v2::tfp1_original::{NativeTfp1PublicOriginalFactsV1, Tfp1PublicOriginalObligationV1},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwTransitionExtrasV1},
    palw_tir_public_source_v1::{PalwTirPublicSourceV1, parameter_cover_v1::StreamingParameterCoverBudgetV1},
};

/// Native numeric and FullIds geometry for a plain original, independent of
/// a legacy Frame and pledge. Selection, DA, and the remaining court controls
/// still need their own obligations in the original planner.
#[allow(dead_code)]
pub(crate) struct PlainBatchOriginalProjectionV2 {
    claim: Hash64,
    source: NativeRetainedV1<PalwTirPublicSourceV1>,
    original_facts: NativeTfp1PublicOriginalFactsV1,
    tokenizer: PlainBatchTokenizerV2,
    accepted_daa: u64,
    prompt: PalwPromptIdsFormV1,
    numeric: NativeRetainedV1<NumericalBatchProjectionV1>,
    output: PlainBatchOutputV2,
}

#[allow(dead_code)]
impl PlainBatchOriginalProjectionV2 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn derive_v2(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        at: &PalwBlockContextV2,
        original: &PalwFreePromptCommitmentV3,
        header: &PalwPublicTirBindingHeaderV1,
        prompt: PalwPromptIdsFormV1,
        maximum_leaves: u64,
        cover: StreamingParameterCoverBudgetV1,
        maximum_ranges: usize,
        verify_cost: ServiceCost,
        verify_daa: u64,
    ) -> Result<Self, NativeDecodeErrorV1<String>> {
        use NativeDecodeErrorV1::Decode;
        let claim = fp_claim_id_v3(original);
        let class = original.job.class_id;
        crate::palw_tir_container_v1::palw_tir_bound_container_v1(parent, class)
            .ok_or_else(|| Decode("plain Batch original has no accepted container".into()))?;
        if parent.claim(&claim).is_some() || parent.class(&class).is_none_or(|class| class.artifact_root != header.artifact_root) {
            return Err(Decode("plain Batch original substitutes accepted claim or artifact".into()));
        }
        let record = parent.tir_class_v1(&class).ok_or_else(|| Decode("plain Batch original class absent".into()))?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(parent, class)
            .ok_or_else(|| Decode("plain Batch original layout absent".into()))?;
        let source = PalwTirPublicSourceV1::new_native_v1(
            record,
            layout,
            header,
            original.execution_root,
            original.trace_root,
            maximum_leaves,
        )?;
        let tokenizer = PlainBatchTokenizerV2::derive_v2(parent, extras, at, original, &source, prompt).map_err(Decode)?;
        let original_facts = NativeTfp1PublicOriginalFactsV1::derive_v1(original, &source)
            .map_err(|why| Decode(format!("plain Batch TFP1 facts: {why:?}")))?;
        let output =
            PlainBatchOutputV2::derive_v2(original, &source, verify_cost, verify_daa).map_err(|why| Decode(format!("{why:?}")))?;
        let numeric = numerical_batch_projection_v1(claim, prompt, &source, record, layout, cover, maximum_ranges)?;
        Ok(Self { claim, source, original_facts, tokenizer, accepted_daa: at.daa_score, prompt, numeric, output })
    }

    fn compile_v2(
        &self,
        scope: Hash64,
        material_until_daa: u64,
        costs: NativeBatchDeliveryCostsV1,
        tokenizer_budget: PlainTokenizerBatchBudgetV2,
        storage: MaterialIntakeScheduleV1,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<NativeRetainedV1<CompiledBatchAdmissionV1>, BatchCourtErrorV1> {
        let shape = NativeBatchDeliveryShapeV1 {
            claim: self.claim,
            scope,
            accepted_daa: self.accepted_daa,
            material_until_daa,
            maximums: self.numeric.maximums_v1(),
        };
        compile_native_batch_plain_tokenizer_v2(shape, &self.output, &self.tokenizer, tokenizer_budget, costs, storage, price)
    }

    fn resource_id_v2(&self, relative_graph: Hash64) -> Result<Hash64, BatchCourtErrorV1> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-batch-original-resource/tokenizer-v2",
            &(self.claim, self.source.source_id_v1(), self.tokenizer.branch_id_v2(), relative_graph),
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)
    }

    fn profile_id_v2(&self, service_profile: Hash64) -> Result<Hash64, BatchCourtErrorV1> {
        if service_profile == Hash64::default() {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-batch-tokenizer-profile/v2",
            &(service_profile, self.tokenizer.branch_id_v2()),
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)
    }

    fn original_terms_v2(
        &self,
        profile: Hash64,
        relative_graph: Hash64,
    ) -> Result<(Hash64, Tfp1PublicOriginalObligationV1), BatchCourtErrorV1> {
        let resource = self.resource_id_v2(relative_graph)?;
        let obligation = self.original_facts.service_obligation_v1(profile, resource).map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        Ok((resource, obligation))
    }

    /// Calculate the signature input before a final allocation scope exists.
    /// A successful compilation does not establish capacity or claim eligibility.
    pub(crate) fn prospective_original_terms_v2(
        &self,
        service_profile: Hash64,
        material_until_daa: u64,
        costs: NativeBatchDeliveryCostsV1,
        tokenizer_budget: PlainTokenizerBatchBudgetV2,
        storage: MaterialIntakeScheduleV1,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<(Hash64, Tfp1PublicOriginalObligationV1), BatchCourtErrorV1> {
        let provisional_scope = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-batch-prospective-scope/tokenizer-v2",
            &(self.claim, self.source.source_id_v1(), self.tokenizer.branch_id_v2()),
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        let compiled = self.compile_v2(provisional_scope, material_until_daa, costs, tokenizer_budget, storage, price)?;
        self.original_terms_v2(self.profile_id_v2(service_profile)?, compiled.relative_graph_id_v1())
    }

    /// Both quote and fold must compile under the final scope before trying
    /// capacity. No provisional slot, assumed custody or inferred bond is reused.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reserve_v2(
        &self,
        book: &mut ServiceReservations,
        plan: &crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1,
        service_profile: Hash64,
        resource_id: Hash64,
        obligation: Tfp1PublicOriginalObligationV1,
        costs: NativeBatchDeliveryCostsV1,
        tokenizer_budget: PlainTokenizerBatchBudgetV2,
        storage: MaterialIntakeScheduleV1,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<(BatchAdmissionQuoteV1, NativeRetainedV1<Vec<CausalServicePacketV1>>), BatchCourtErrorV1> {
        if (plan.original_claim, plan.source_id, plan.terms.accepted_daa)
            != (self.claim, self.source.source_id_v1(), self.accepted_daa)
        {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        let profile = self.profile_id_v2(service_profile)?;
        let (scope, _) = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::scope_from_native_inputs_v2(
            &self.source,
            self.claim,
            plan.allocation_id,
            plan.rules_id,
            profile,
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        let compiled = self.compile_v2(scope, plan.terms.material_until_daa, costs, tokenizer_budget, storage, price)?;
        if self.original_terms_v2(profile, compiled.relative_graph_id_v1())? != (resource_id, obligation) {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        CompiledBatchAdmissionV1::reserve_v1(compiled, book)
    }
}
