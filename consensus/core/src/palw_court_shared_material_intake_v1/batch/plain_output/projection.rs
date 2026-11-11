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
                NativeBatchDeliveryShapeV1, compile_native_batch_plain_selection_v3, compile_native_batch_plain_tokenizer_v2,
                plain_selection::{PlainSelectionBranchV3, PlainSelectionBudgetV3},
                plain_tokenizer::PlainTokenizerBatchBudgetV2,
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
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1},
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
    selection: Option<PlainSelectionBranchV3>,
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
    pub(crate) fn accepted_daa_v2(&self) -> u64 {
        self.acceptance_clocks.0
    }
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
    /// Claim-specific Trace payload, excluding transport/opening overhead.
    /// This is a lower bound on a protected replay opportunity, not its full
    /// service quote. It never includes HF model weights.
    pub(crate) fn public_trace_payload_bytes_v1(&self) -> Result<u64, BatchCourtErrorV1> {
        self.source
            .trace_value_bytes_v1()
            .and_then(|bytes| bytes.checked_add(crate::palw_tir_court_v1::PublicOriginalTraceIndexV1::WIRE_BYTES as u64))
            .ok_or(BatchCourtErrorV1::CaseGraph)
    }

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
            selection: None,
        })
    }

    /// Extend this exact prospective original using the current native
    /// custody bound. It is not the drawn-claimant count. The accepting owner
    /// must still bind the signed custody contract and implement all protected
    /// notice/row/repair consumers before enabling the complete profile.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_native_selection_v3(
        mut self,
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        params: &PalwStateParamsV2,
        at: &PalwBlockContextV2,
        original: &PalwFreePromptCommitmentV3,
        policy: PlainSelectionBudgetV3,
    ) -> Result<Self, NativeDecodeErrorV1<String>> {
        use NativeDecodeErrorV1::Decode;
        if self.selection.is_some()
            || self.acceptance_clocks != (at.daa_score, at.blue_score)
            || self.claim != fp_claim_id_v3(original)
            || parent.claim(&self.claim).is_some()
        {
            return Err(Decode("Selection projection substitutes original or prospective clocks".into()));
        }
        // The legacy helper has a conservative sentinel for absent registry.
        // A new profile must require actual native role data, not price that
        // sentinel as if it described a real custody population.
        if !params.panel_v3_rule_at(at.daa_score) && extras.model_registry.is_none() {
            return Err(Decode("Selection projection has no native custody policy".into()));
        }
        let tokenizer = PlainBatchTokenizerV2::derive_v2(parent, extras, at, original, &self.source, self.prompt).map_err(Decode)?;
        if tokenizer != self.tokenizer {
            return Err(Decode("Selection projection changes the authenticated parent tokenizer registration".into()));
        }
        let roles = crate::palw_tir_frame_cost_v1::palw_tir_frame_custodian_upper_v1(at.daa_score, params, extras)
            .map_err(|why| Decode(format!("Selection native custody: {why:?}")))?;
        let record = parent.tir_class_v1(&original.job.class_id).ok_or_else(|| Decode("Selection class absent".into()))?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(parent, original.job.class_id)
            .ok_or_else(|| Decode("Selection layout absent".into()))?;
        self.selection = Some(PlainSelectionBranchV3::derive_v3(original, &self.source, record, layout, roles, policy)?);
        Ok(self)
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
        match &self.selection {
            Some(selection) => compile_native_batch_plain_selection_v3(
                shape,
                &self.output,
                &self.tokenizer,
                tokenizer_budget,
                selection,
                costs,
                storage,
                price,
            ),
            None => {
                compile_native_batch_plain_tokenizer_v2(shape, &self.output, &self.tokenizer, tokenizer_budget, costs, storage, price)
            }
        }
    }

    fn resource_id_v2(&self, relative_graph: Hash64) -> Result<Hash64, BatchCourtErrorV1> {
        if let Some(selection) = &self.selection {
            return crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/plain-batch-original-resource/selection-v3",
                &(self.claim, self.source.source_id_v1(), self.tokenizer.branch_id_v2(), selection.identity_v3()?, relative_graph),
            )
            .map_err(|_| BatchCourtErrorV1::CaseGraph);
        }
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
        if let Some(selection) = &self.selection {
            return crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/plain-batch-selection-profile/v3",
                &(service_profile, self.tokenizer.branch_id_v2(), selection.identity_v3()?),
            )
            .map_err(|_| BatchCourtErrorV1::CaseGraph);
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
    params: &PalwStateParamsV2,
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
    crate::palw_positive_contract_v2::tfp1_original::preclaim::tests::exercise_preclaim_booking_v1(
        &projection,
        terms,
        fixture,
        at,
        false,
    );

    // Explicit local custody policy. The branch derives its role count from
    // this native input, never from the selected-claimant rescue count.
    let mut selection_extras = extras.clone();
    let mut globals = crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = 2;
    selection_extras.model_registry = Some(crate::palw_model_registry_v1::PalwModelRegistryFoldV1 {
        globals,
        span_daa: 600,
        genesis_works: Default::default(),
        grace_until_daa: 0,
        admission_audit_period_daa: None,
        readiness_v2_active: false,
        bond_maturity: None,
    });
    let selection_policy = PlainSelectionBudgetV3 {
        continuation:
            crate::palw_court_shared_material_intake_v1::batch::admission::plain_selection::SelectionContinuationBudgetV3::test_only_v3(
            ),
        verification: ServiceCost { bytes: 1_000_000, work: 5_000_000, io: 5_000_000, controls: 1 },
        verification_daa: 1,
        workspace_bytes: 128 << 20,
        maximum_proof_bytes: 1_000_000,
        maximum_dependencies: 4096,
        maximum_ranges_per_role: 512,
        maximum_material_visits: 1_000_000,
    };
    let make_selection = |extras: &PalwTransitionExtrasV1| {
        derive(at).with_native_selection_v3(parent, extras, params, at, fixture.frame.original_v1(), selection_policy)
    };
    let selected = make_selection(&selection_extras).unwrap();
    assert_eq!(selected.selection.as_ref().unwrap().role_count_v3(), 3);
    let mut absent_policy = selection_extras.clone();
    absent_policy.model_registry = None;
    assert!(make_selection(&absent_policy).is_err());
    let selection_costs = NativeBatchDeliveryCostsV1 {
        terminal: ServiceCost {
            bytes: 2_000_000,
            work: costs.terminal.work + selection_policy.verification.work,
            io: costs.terminal.io + selection_policy.verification.io,
            controls: 1,
        },
        ..costs
    };
    let selected_terms = selected
        .compile_prospective_original_terms_v2(profile, at.daa_score + 11_000, selection_costs, tokenizer_budget, storage, quote_price)
        .unwrap();
    assert_ne!(selected_terms.profile_id_v2(), terms.profile_id_v2());
    assert_ne!(selected_terms.resource_id_v2(), terms.resource_id_v2());
    let selected_book =
        ServiceReservations::new(ServiceCost { bytes: 100_000_000, work: 100_000_000, io: 100_000_000, controls: 4096 }).unwrap();
    let mut booked = selected_book.clone();
    let (selected_quote, graph) = selected.reserve_before_allocation_v2(&mut booked, a, &selected_terms, quote_price).unwrap();
    assert!(selected_quote.service.bytes > first.service.bytes);
    assert!(selected_quote.retained_payload_bytes > first.retained_payload_bytes);
    let branch = selected.selection.as_ref().unwrap();
    let binding = branch.binding_id_v3(selected.claim, a).unwrap();
    let bind_node = graph.iter().find(|node| node.packet.id == binding).unwrap();
    assert_eq!(bind_node.packet.cost, selection_policy.continuation.bind);
    assert_eq!(bind_node.packet.release_daa, at.daa_score + selection_policy.continuation.ready_offset_daa);
    assert!(bind_node.packet.release_daa > at.daa_score + crate::palw_court_policy_v1::NOTICE_WINDOW_DAA);
    for role in 0..branch.role_count_v3() {
        let mut index = 0;
        while let Ok(ordinal) = branch.demand_ordinal_v3(role, index) {
            let id = crate::palw_batch_court_v1::rooted::batch_open_packet_v1(selected.claim, a, ordinal).unwrap().as_bytes();
            let open = graph.iter().find(|node| node.packet.id == id).unwrap();
            assert_eq!(open.predecessors, vec![binding]);
            assert!(open.packet.release_daa >= bind_node.packet.service_before);
            assert!(booked.assigned_daa(&binding).unwrap() < booked.assigned_daa(&id).unwrap());
            index += 1;
        }
        assert!(index > 0);
    }
    let verification = branch.verification_id_v3(selected.claim, a).unwrap();
    let node = graph.iter().find(|node| node.packet.id == verification).unwrap();
    assert!(!node.predecessors.is_empty());
    for predecessor in &node.predecessors {
        assert!(booked.assigned_daa(predecessor).unwrap() < booked.assigned_daa(&verification).unwrap());
    }
    let (_, terminal) =
        crate::palw_court_shared_material_intake_v1::batch::admission::batch_numerical_service_ids_v1(selected.claim, a).unwrap();
    assert!(graph.iter().find(|node| node.packet.id == terminal).unwrap().predecessors.contains(&verification));
    assert!(booked.assigned_daa(&verification).unwrap() < booked.assigned_daa(&terminal).unwrap());
    let mut refused = selected_book.clone();
    assert!(selected.reserve_before_allocation_v2(&mut refused, a, &terms, quote_price).is_err());
    assert_eq!(refused, selected_book);
    assert!(
        selected
            .compile_prospective_original_terms_v2(profile, at.daa_score + 11_000, costs, tokenizer_budget, storage, quote_price,)
            .is_err(),
        "new terminal must fund Selection recheck too"
    );
    crate::palw_positive_contract_v2::tfp1_original::preclaim::tests::exercise_preclaim_booking_v1(
        &selected,
        selected_terms,
        fixture,
        at,
        true,
    );
}
