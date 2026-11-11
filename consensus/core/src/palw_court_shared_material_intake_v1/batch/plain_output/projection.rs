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
                NativeBatchDeliveryShapeV1, compile_native_batch_plain_output_v2,
            },
            native_ranges::{NumericalBatchProjectionV1, numerical_batch_projection_v1},
        },
    },
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwChainStateV2, PalwTransitionExtrasV1},
    palw_tir_public_source_v1::{PalwTirPublicSourceV1, parameter_cover_v1::StreamingParameterCoverBudgetV1},
};

/// Native numeric and FullIds geometry for a plain original, independent of
/// a legacy Frame and pledge. Selection, DA, and the remaining court controls
/// still need their own obligations in the original planner.
#[allow(dead_code)]
pub(crate) struct PlainBatchOriginalProjectionV2 {
    claim: Hash64,
    source: NativeRetainedV1<PalwTirPublicSourceV1>,
    registration: Hash64,
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
        accepted_daa: u64,
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
        let registration = crate::palw_tokenizer_rendering_v2::palw_rooted_tokenizer_opening_bounds_v1(
            parent,
            extras,
            accepted_daa,
            original,
            header,
        )
        .ok_or_else(|| Decode("plain Batch original requires an earlier authenticated tokenizer registration".into()))?;
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
        let output =
            PlainBatchOutputV2::derive_v2(original, &source, verify_cost, verify_daa).map_err(|why| Decode(format!("{why:?}")))?;
        let numeric = numerical_batch_projection_v1(claim, prompt, &source, record, layout, cover, maximum_ranges)?;
        Ok(Self { claim, source, registration: registration.registration_id, accepted_daa, prompt, numeric, output })
    }

    fn compile_v2(
        &self,
        scope: Hash64,
        material_until_daa: u64,
        costs: NativeBatchDeliveryCostsV1,
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
        compile_native_batch_plain_output_v2(shape, &self.output, costs, storage, price)
    }

    fn resource_id_v2(&self, relative_graph: Hash64) -> Result<Hash64, BatchCourtErrorV1> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-batch-original-resource/v2",
            &(self.claim, self.source.source_id_v1(), self.registration, relative_graph),
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)
    }

    /// Calculate the signature input before a final allocation scope exists.
    /// A successful compilation does not establish capacity or claim eligibility.
    pub(crate) fn prospective_resource_id_v2(
        &self,
        material_until_daa: u64,
        costs: NativeBatchDeliveryCostsV1,
        storage: MaterialIntakeScheduleV1,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<Hash64, BatchCourtErrorV1> {
        let provisional_scope = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-batch-prospective-scope/v2",
            &(self.claim, self.source.source_id_v1(), self.registration),
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        let compiled = self.compile_v2(provisional_scope, material_until_daa, costs, storage, price)?;
        self.resource_id_v2(compiled.relative_graph_id_v1())
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
        costs: NativeBatchDeliveryCostsV1,
        storage: MaterialIntakeScheduleV1,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<(BatchAdmissionQuoteV1, NativeRetainedV1<Vec<CausalServicePacketV1>>), BatchCourtErrorV1> {
        if (plan.original_claim, plan.source_id, plan.terms.accepted_daa)
            != (self.claim, self.source.source_id_v1(), self.accepted_daa)
        {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        let profile = crate::palw_court_shared_tir_state_v1::plain_batch_profile_id_v2(service_profile, self.registration)
            .map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        let (scope, _) = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::scope_from_native_inputs_v2(
            &self.source,
            self.claim,
            plan.allocation_id,
            plan.rules_id,
            profile,
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        let compiled = self.compile_v2(scope, plan.terms.material_until_daa, costs, storage, price)?;
        if self.resource_id_v2(compiled.relative_graph_id_v1())? != resource_id {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        CompiledBatchAdmissionV1::reserve_v1(compiled, book)
    }
}
