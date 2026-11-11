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
    acceptance_clocks: (u64, u64),
    prompt: PalwPromptIdsFormV1,
    numeric: NativeRetainedV1<NumericalBatchProjectionV1>,
    output: PlainBatchOutputV2,
}

/// Output of native compilation before an allocation or a final packet
/// namespace exists. This is local planning data. Neither the terms nor a
/// successful book reservation authorize an original or protected case.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlainBatchProspectiveOriginalTermsV2 {
    acceptance_clocks: (u64, u64),
    material_until_daa: u64,
    profile: Hash64,
    relative_graph: Hash64,
    resource: Hash64,
    obligation: Tfp1PublicOriginalObligationV1,
    costs: NativeBatchDeliveryCostsV1,
    tokenizer_budget: PlainTokenizerBatchBudgetV2,
    storage: MaterialIntakeScheduleV1,
}

#[allow(dead_code)]
impl PlainBatchProspectiveOriginalTermsV2 {
    pub(crate) fn matches_acceptance_v2(&self, daa: u64, blue: u64) -> bool {
        self.acceptance_clocks == (daa, blue)
    }

    pub(crate) fn material_until_daa_v2(&self) -> u64 {
        self.material_until_daa
    }

    pub(crate) fn profile_id_v2(&self) -> Hash64 {
        self.profile
    }

    pub(crate) fn resource_id_v2(&self) -> Hash64 {
        self.resource
    }

    pub(crate) fn relative_graph_id_v2(&self) -> Hash64 {
        self.relative_graph
    }

    pub(crate) fn obligation_v2(&self) -> Tfp1PublicOriginalObligationV1 {
        self.obligation
    }
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
        Ok(Self {
            claim,
            source,
            original_facts,
            tokenizer,
            acceptance_clocks: (at.daa_score, at.blue_score),
            prompt,
            numeric,
            output,
        })
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
            accepted_daa: self.acceptance_clocks.0,
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
        let terms =
            self.compile_prospective_original_terms_v2(service_profile, material_until_daa, costs, tokenizer_budget, storage, price)?;
        Ok((terms.resource, terms.obligation))
    }

    /// Both quote and admission start here. Use the authenticated parent
    /// and the actual prospective clocks to derive a fresh projection on
    /// each path. Only relative packet geometry enters the signing terms.
    pub(crate) fn compile_prospective_original_terms_v2(
        &self,
        service_profile: Hash64,
        material_until_daa: u64,
        costs: NativeBatchDeliveryCostsV1,
        tokenizer_budget: PlainTokenizerBatchBudgetV2,
        storage: MaterialIntakeScheduleV1,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<PlainBatchProspectiveOriginalTermsV2, BatchCourtErrorV1> {
        let profile = self.profile_id_v2(service_profile)?;
        let provisional_scope = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/plain-batch-prospective-scope/tokenizer-v2",
            &(self.claim, self.source.source_id_v1(), self.tokenizer.branch_id_v2()),
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        let compiled = self.compile_v2(provisional_scope, material_until_daa, costs, tokenizer_budget, storage, price)?;
        let relative_graph = compiled.relative_graph_id_v1();
        let (resource, obligation) = self.original_terms_v2(profile, relative_graph)?;
        Ok(PlainBatchProspectiveOriginalTermsV2 {
            acceptance_clocks: self.acceptance_clocks,
            material_until_daa,
            profile,
            relative_graph,
            resource,
            obligation,
            costs,
            tokenizer_budget,
            storage,
        })
    }

    /// Compile the physical graph in an independently resolved native packet
    /// namespace, e.g. a prebooked rank. Callers own namespace derivation
    /// and can pass the result to the atomic multi-case book reservation.
    /// Compare relative identity before spending any book capacity.
    pub(crate) fn compile_before_allocation_v2(
        &self,
        packet_namespace: Hash64,
        terms: &PlainBatchProspectiveOriginalTermsV2,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<NativeRetainedV1<CompiledBatchAdmissionV1>, BatchCourtErrorV1> {
        if terms.acceptance_clocks != self.acceptance_clocks
            || self.original_terms_v2(terms.profile, terms.relative_graph)? != (terms.resource, terms.obligation)
        {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        let compiled = self.compile_checked_terms_v2(
            packet_namespace,
            terms.material_until_daa,
            terms.profile,
            (terms.resource, terms.obligation),
            terms.costs,
            terms.tokenizer_budget,
            terms.storage,
            price,
        )?;
        if compiled.relative_graph_id_v1() != terms.relative_graph {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        Ok(compiled)
    }

    /// Quote capacity on a private book, or reserve it for admission. A
    /// protected set must instead compile all its ranks then reserve them
    /// together. No legacy allocation is required on either path.
    pub(crate) fn reserve_before_allocation_v2(
        &self,
        book: &mut ServiceReservations,
        packet_namespace: Hash64,
        terms: &PlainBatchProspectiveOriginalTermsV2,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<(BatchAdmissionQuoteV1, NativeRetainedV1<Vec<CausalServicePacketV1>>), BatchCourtErrorV1> {
        let compiled = self.compile_before_allocation_v2(packet_namespace, terms, price)?;
        CompiledBatchAdmissionV1::reserve_v1(compiled, book)
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_checked_terms_v2(
        &self,
        packet_namespace: Hash64,
        material_until_daa: u64,
        profile: Hash64,
        expected: (Hash64, Tfp1PublicOriginalObligationV1),
        costs: NativeBatchDeliveryCostsV1,
        tokenizer_budget: PlainTokenizerBatchBudgetV2,
        storage: MaterialIntakeScheduleV1,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<NativeRetainedV1<CompiledBatchAdmissionV1>, BatchCourtErrorV1> {
        let compiled = self.compile_v2(packet_namespace, material_until_daa, costs, tokenizer_budget, storage, price)?;
        if self.original_terms_v2(profile, compiled.relative_graph_id_v1())? != expected {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        Ok(compiled)
    }

    /// Compatibility wrapper for the original Frame-backed namespace. Keep
    /// the exact native plan identity and signing terms checks.
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
            != (self.claim, self.source.source_id_v1(), self.acceptance_clocks.0)
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
        let compiled = self.compile_checked_terms_v2(
            scope,
            plan.terms.material_until_daa,
            profile,
            (resource_id, obligation),
            costs,
            tokenizer_budget,
            storage,
            price,
        )?;
        CompiledBatchAdmissionV1::reserve_v1(compiled, book)
    }
}

/// Called from the native tokenizer fixture once the registration is present
/// on its prospective parent. No accepted original frame or plan is passed to
/// either booking API.
#[cfg(test)]
pub(crate) fn exercise_batch_booking_before_allocation_v2(
    parent: &PalwChainStateV2,
    extras: &PalwTransitionExtrasV1,
    at: &PalwBlockContextV2,
    fixture: &crate::palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1,
) {
    use crate::{
        palw_court_shared_material_intake_v1::batch::admission::NativeBatchDeliveryTimingV1,
        palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
    };

    let cover = OriginalParameterCoverBudgetV1 {
        max_original_leaves: 4096,
        max_total_edges: 1_000_000,
        max_total_coordinates: 1_000_000,
        max_root_memberships: 1_000_000,
    };
    let storage = MaterialIntakeScheduleV1 { sompi_per_byte: 2, draft_ttl_daa: 32, cover };
    let tokenizer_budget = PlainTokenizerBatchBudgetV2 {
        verification: ServiceCost { bytes: 20_000, work: 1_000_000, io: 1_000_000, controls: 1 },
        workspace_bytes: 1 << 20,
        verification_daa: 1,
    };
    let numerical_verification = crate::palw_court_shared_material_intake_v1::NumericalBatchReplayBudgetV1::test_only_v1();
    let numerical = numerical_verification.service_cost_v1().unwrap();
    let costs = NativeBatchDeliveryCostsV1 {
        numerical_verification,
        terminal: ServiceCost {
            bytes: 100_000,
            work: numerical.work + tokenizer_budget.verification.work,
            io: numerical.io + tokenizer_budget.verification.io,
            controls: 1,
        },
        recovery_daa: 1,
        maximum_packets: 4096,
        timing: NativeBatchDeliveryTimingV1 {
            open_daa: 1,
            provider_wait_daa: 1,
            begin_daa: 1,
            parts_daa: 8,
            finish_daa: 1,
            verification_daa: 1,
            stop_daa: 1,
            terminal_daa: 1,
        },
    };
    let derive = |at: &PalwBlockContextV2| {
        PlainBatchOriginalProjectionV2::derive_v2(
            parent,
            extras,
            at,
            fixture.frame.original_v1(),
            fixture.frame.header_v1(),
            PalwPromptIdsFormV1::Flat,
            fixture.rules.maximum_leaves,
            StreamingParameterCoverBudgetV1 {
                total: cover,
                per_root_edges: cover.max_total_edges,
                per_root_coordinates: cover.max_total_coordinates,
                per_root_memberships: cover.max_root_memberships,
                max_workspace_bytes: 128 << 20,
            },
            512,
            tokenizer_budget.verification,
            1,
        )
        .unwrap()
    };
    let quote_price = |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
    let altered_price = |_, bytes| Ok(ServiceCost { bytes, work: 2, io: 1, controls: 1 });
    let profile = Hash64::from_u64_word(830);
    let a = Hash64::from_u64_word(831);
    let b = Hash64::from_u64_word(832);
    let projection = derive(at);
    let terms_for = |projection: &PlainBatchOriginalProjectionV2, daa: u64| {
        projection.compile_prospective_original_terms_v2(profile, daa + 11_000, costs, tokenizer_budget, storage, quote_price).unwrap()
    };
    let terms = terms_for(&projection, at.daa_score);
    let original_book = ServiceReservations::new(ServiceCost {
        bytes: 20_000_000,
        work: numerical.work + 10_000_000,
        io: numerical.io + 10_000_000,
        controls: 256,
    })
    .unwrap();
    let mut first_book = original_book.clone();
    let (first, first_graph) = projection.reserve_before_allocation_v2(&mut first_book, a, &terms, quote_price).unwrap();
    let mut second_book = original_book.clone();
    let (second, second_graph) = projection.reserve_before_allocation_v2(&mut second_book, b, &terms, quote_price).unwrap();
    assert_eq!((first.scope, second.scope), (a, b));
    assert_eq!(first.relative_graph_id, terms.relative_graph_id_v2());
    assert_eq!(second.relative_graph_id, terms.relative_graph_id_v2());
    assert_ne!(first_graph[0].packet.id, second_graph[0].packet.id);

    let mut unchanged = original_book.clone();
    assert!(matches!(
        projection.reserve_before_allocation_v2(&mut unchanged, a, &terms, altered_price),
        Err(BatchCourtErrorV1::CaseGraph)
    ));
    let rebased = derive(&PalwBlockContextV2 { blue_score: at.blue_score + 1, ..*at });
    assert!(matches!(rebased.reserve_before_allocation_v2(&mut unchanged, a, &terms, quote_price), Err(BatchCourtErrorV1::CaseGraph)));
    assert_eq!(unchanged, original_book);
    let rebased_terms = terms_for(&rebased, at.daa_score);
    assert_eq!(rebased_terms.obligation_v2(), terms.obligation_v2());
    assert_eq!(rebased_terms.resource_id_v2(), terms.resource_id_v2());
    rebased.reserve_before_allocation_v2(&mut unchanged, a, &rebased_terms, quote_price).unwrap();
    assert_eq!(unchanged, first_book);
    crate::palw_positive_contract_v2::tfp1_original::preclaim::tests::exercise_preclaim_booking_v1(&projection, terms, fixture, at);
}
