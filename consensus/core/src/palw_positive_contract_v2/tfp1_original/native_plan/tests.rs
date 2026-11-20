//! Called by the existing registered-tokenizer/container fixture. These
//! generous local values exercise assembly, not a shipped public profile.
use super::*;
use crate::{palw_state_v2::PalwBlockContextV2, palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1};
use misaka_palw_positive_contract::{
    identity::ChainScope,
    public_one_honest::{
        HfArtifactNativePinsV1, ManifestScopeRules, hf_artifact_material_codec_v1, prompt_text_material_codec_v1,
        prompt_text_material_v1, tir_cold_dependencies_codec_v1, tir_cold_initial_state_codec_v1,
        tir_ordinary_tfp1_rules_material_codec_v1, tir_output_material_codec_v1, tir_program_material_codec_v1,
        tir_trace_material_codec_v1, tokenizer_json_material_codec_v1,
    },
    rows::NativeHash64,
};

/// Complete configuration for this retained, small fixture only. The capacity
/// is the component-wise interval floor of its 152-packet native graph (three
/// 34,000-unit packets share [3124,3125)); it is NOT a Studio/model profile.
/// Cover bounds copy the same fixture's already exercised streaming policy,
/// including its 128 MiB workspace, rather than mixing that workspace with
/// the pinned profile's much larger per-root limits. Other terms copy pinned
/// preimages. Host-local memory availability never enters its identity.
fn private_fixture_declaration_v3(
    cover: StreamingParameterCoverBudgetV1,
) -> super::super::public_policy::ProtectedPlanningDeclarationV3 {
    let book = crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1.unwrap();
    let batch = crate::palw_g14_values_v1::palw_g14_funded_batch_schedule_estimate_v1();
    let material = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().unwrap();
    let funding = crate::palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1();
    let mut replay = batch.delivery.numerical_verification;
    replay.meter_checker_io = true;
    let replay_cost = replay.service_cost_v1().unwrap();
    let terminal = ServiceCost { work: replay_cost.work, ..batch.delivery.terminal };
    let timing = batch.delivery.timing;
    let cost = |value: ServiceCost| {
        serde_json::json!({
            "bytes": value.bytes, "work": value.work, "io": value.io, "controls": value.controls,
        })
    };
    let value = serde_json::json!({
        "version": 3,
        "service_book": {
            "budget": {"bytes": 102_000, "work": 102_000, "io": 102_000, "controls": 3},
            "packet_cost": cost(book.packet_cost), "service_span_daa": book.service_span_daa,
        },
        "cover": {
            "max_original_leaves": cover.total.max_original_leaves,
            "max_total_edges": cover.total.max_total_edges,
            "max_total_coordinates": cover.total.max_total_coordinates,
            "max_root_memberships": cover.total.max_root_memberships,
            "per_root_edges": cover.per_root_edges,
            "per_root_coordinates": cover.per_root_coordinates,
            "per_root_memberships": cover.per_root_memberships,
            "maximum_ranges": batch.maximum_ranges, "workspace_bytes": cover.max_workspace_bytes,
        },
        "batch": {
            "numerical_replay": {
                "max_elements": replay.work.evaluation.max_elements, "max_terms": replay.work.evaluation.max_terms,
                "max_target_requests": replay.work.max_target_requests, "max_source_reads": replay.work.max_source_reads,
                "workspace_bytes": replay.workspace_bytes, "max_fault_bytes": replay.max_fault_bytes,
                "base_work": replay.base_work, "io": replay.io,
            },
            "terminal": cost(terminal), "recovery_daa": batch.delivery.recovery_daa,
            "maximum_packets": batch.delivery.maximum_packets,
            "timing": {
                "open_daa": timing.open_daa, "provider_wait_daa": timing.provider_wait_daa,
                "begin_daa": timing.begin_daa, "parts_daa": timing.parts_daa,
                "finish_daa": timing.finish_daa, "verification_daa": timing.verification_daa,
                "stop_daa": timing.stop_daa, "terminal_daa": timing.terminal_daa,
            },
            "open_carrier": cost(batch.carrier_costs[0]), "begin_carrier": cost(batch.carrier_costs[1]),
            "part_carrier": cost(batch.carrier_costs[2]), "finish_carrier": cost(batch.carrier_costs[3]),
            "open_work_per_range": batch.open_work_per_range, "finish_work_per_byte": batch.finish_work_per_byte,
            "finish_io_per_byte": batch.finish_io_per_byte,
            "stop_verification": cost(batch.stop_verification), "stop_filing": cost(batch.stop_filing),
        },
        "material": {"sompi_per_byte": material.sompi_per_byte, "draft_ttl_daa": material.draft_ttl_daa},
        "funding": {
            "original_fee_sompi": funding.fee_sompi(), "capital_floor_sompi": funding.capital_floor_sompi(),
            "maximum_dependencies": funding.source_budget().max_queries,
            "maximum_material_visits": funding.source_budget().max_vertices,
        },
        "packet_carriage_sompi": crate::palw_g14_values_v1::PALW_G14_PACKET_CARRIAGE_SOMPI_V1,
        "rounds": book.packets_per_signer,
    });
    super::super::public_policy::ProtectedPlanningDeclarationV3::decode_json_v3(&serde_json::to_vec(&value).unwrap()).unwrap()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn exercise_native_plan_v3(
    parent: &PalwChainStateV2,
    extras: &PalwTransitionExtrasV1,
    params: &PalwStateParamsV2,
    at: &PalwBlockContextV2,
    fixture: &PalwTirPublicFrameFixtureV1,
    selection: PlainSelectionBudgetV3,
    delivery: NativeBatchDeliveryCostsV1,
    tokenizer: PlainTokenizerBatchBudgetV2,
    storage: MaterialIntakeScheduleV1,
) {
    use super::super::preclaim::original_duties::OriginalEarlyDutyCostV1;
    let mut retained_fp = fixture.frame.original_v1().clone();
    // This planner fixture promises enough material life for a delayed
    // acceptance. The execution/header stay the same; claim identity is
    // derived from this complete, changed original everywhere below.
    retained_fp.trace_retention_daa = at.daa_score + 12_064;
    let fp = &retained_fp;
    let header = fixture.frame.header_v1();
    let container = crate::palw_tir_container_v1::palw_tir_bound_container_v1(parent, fp.job.class_id).unwrap();
    let descriptor = HfArtifactDescriptorV1::new_v1(
        HfArtifactNativePinsV1 {
            class: Digest(fp.job.class_id.as_bytes()),
            program: Digest(fixture.record.graph_ir_root.as_bytes()),
            tokenizer: Digest(fixture.record.tokenizer_id.as_bytes()),
            inventory: Digest(header.artifact_root.as_bytes()),
            container: Digest(container.container_root.as_bytes()),
            binding_manifest: Digest(container.manifest_digest.as_bytes()),
        },
        "HuggingFaceTB/SmolLM2-135M",
        [1; 20],
        Digest([2; 64]),
        Digest([3; 64]),
    )
    .unwrap();
    let descriptor_bytes = descriptor.encode_v1().unwrap();
    let form = PalwPromptIdsFormV1::Flat;
    let codecs = [
        hf_artifact_material_codec_v1(),
        tir_program_material_codec_v1(),
        prompt_text_material_codec_v1(),
        Digest(crate::palw_prompt_ids_v1::prompt_ids_form_id_v1(form).as_bytes()),
        tokenizer_json_material_codec_v1(),
        tir_ordinary_tfp1_rules_material_codec_v1(),
        tir_cold_initial_state_codec_v1(),
        tir_cold_dependencies_codec_v1(),
        tir_trace_material_codec_v1(),
        tir_output_material_codec_v1(),
    ];
    let limits = codecs.map(|codec| ManifestScopeRules { codec, max_bytes: 100_000_000, chunk_bytes: 1 << 20 });
    let material_ceiling = 1_000_000_000 + u64::from(ManifestV1::WIRE_BYTES);
    let allowance =
        RemainingCapacity { transmitted_bytes: material_ceiling, retained_bytes: material_ceiling, additional_work: 1_000_000_000 };
    // Initial material and each role's case are simultaneous obligations.
    // Leave room for the case instead of assigning the entire role allowance
    // to initial material alone.
    let initial_supply = allowance;
    let role_allowance = allowance.checked_scale(2).unwrap();
    let rescue = ProtectedRescueDemand {
        max_drawn_bonds: 2,
        per_drawn_bond: role_allowance,
        max_outsiders: 1,
        per_outsider: role_allowance,
        defensive: role_allowance,
    };
    let total = initial_supply.checked_add(rescue.total().unwrap()).unwrap();
    let scope = ChainScope { network: Digest(fp.job.network_domain.as_bytes()), ruleset: Digest([91; 64]), branch: Digest([92; 64]) };
    let request_retained = InitialMaterialRequestPlanV1::minimum_retained_bytes_v1();
    let notice_bytes = crate::palw_court_shared_material_intake_v1::batch::selection::notice::SignedSelectionNoticeV3::BYTES;
    let policy = Tfp1NativePlanningPolicyV3 {
        relative_service_v2: true,
        admission: Tfp1PublicAdmissionPolicyV1 {
            scope,
            max_manifest_bytes: ManifestV1::WIRE_BYTES,
            manifest_scope_limits: &limits,
            manifest_total_limit: 1_000_000_000,
        },
        maximum_leaves: fixture.rules.maximum_leaves,
        cover: StreamingParameterCoverBudgetV1 {
            total: storage.cover,
            // Match this fixture's streamed projection: cumulative work
            // allowances are not simultaneously resident per-root buffers.
            per_root_edges: 4096,
            per_root_coordinates: 4096,
            per_root_memberships: 4096,
            max_workspace_bytes: 128 << 20,
        },
        maximum_ranges: 512,
        output_verification: tokenizer.verification,
        output_verification_daa: 1,
        selection,
        service_book_profile: crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1.unwrap(),
        service_profile: Hash64::from_u64_word(830),
        delivery,
        tokenizer,
        storage,
        early: ProtectedEarlyDutyPolicyV1 {
            first_notice_wire_bytes: notice_bytes,
            first_notice: OriginalEarlyDutyCostV1 {
                service: ServiceCost { bytes: notice_bytes, work: notice_bytes, io: notice_bytes, controls: 1 },
                retained_bytes: notice_bytes,
                escrow_sompi: 10,
                future_carrier_sompi: 20,
            },
        },
        request_service: ServiceCost { bytes: 1_000_000, work: 1_000_000, io: 1_000_000, controls: 1 },
        request_retained_bytes: request_retained,
        request_max_gap_daa: 256,
        notice: FirstNoticeDemand {
            draw_ready_by: at.daa_score + 1,
            material_ready_by: at.daa_score + 1,
            cold_acquisition_daa: 1,
            full_inspection_daa: 1,
            notice_inclusion_daa: 1,
            missing_material_detected_by: at.daa_score + 1,
            material_request_inclusion_daa: 1,
        },
        case: MaterialCaseDemand {
            material_delivery_daa: 1_000,
            full_inspection_daa: 1_000,
            localization_daa: 1_000,
            filing_daa: 1_000,
            adjudication_and_recovery_daa: 1_000,
        },
        initial_supply,
        rescue,
        budget: AggregateBudget {
            rounds: 100,
            unique_source_bytes: material_ceiling,
            transmitted_bytes: total.transmitted_bytes,
            additional_work: total.additional_work,
            retained_bytes: total.retained_bytes,
            retention_until: at.daa_score + 12_000,
            fee: 1_000_000_000_000_000,
            witness_attempts: 100,
        },
        usage: AggregateUsage::default(),
        funding: Tfp1FundingPolicyV1 {
            profiles: [Hash64::from_u64_word(930); 4],
            other_relative_graph: Hash64::from_u64_word(931),
            original_fee_sompi: 100,
            capital_floor_sompi: 1_000,
        },
        maximum_packets: 32_768,
    };
    let price = |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
    let derive = |bytes: &[u8], blue| {
        NativeTfp1OriginalPlanV3::derive_v3(
            parent,
            extras,
            params,
            at.daa_score,
            blue,
            fp,
            header,
            form,
            b"fixture prompt",
            bytes,
            &policy,
            price,
        )
    };
    let plan = derive(&descriptor_bytes, at.blue_score).unwrap();
    assert_eq!(plan.source_v3().header_v1(), header);
    let (supply_identity, manifest_body) = plan.terms_v3().manifest_supply_v3().unwrap();
    assert_ne!(supply_identity, Digest([0; 64]));
    assert_eq!(manifest_body, plan.supply.manifest_body_bytes_v1());
    // Notice-end slots share one rank. A single initial request is not the series.
    assert!(plan.terms_v3().request_count_v2() > 1);
    let admission = plan.admission_v3(fp, b"fixture prompt").unwrap();
    let job = admission.job;
    let subjects = [
        Digest(admission.artifact.as_bytes()),
        Digest(plan.class.graph_ir_root().as_bytes()),
        admission.prompt,
        Digest(job.prompt_token_ids_hash.as_bytes()),
        Digest(job.tokenizer_id.as_bytes()),
        Digest(job.cu_ruleset_id.as_bytes()),
        admission.initial_state,
        admission.dependencies,
        Digest(fp.execution_root.as_bytes()),
        Digest(fp.output_root.as_bytes()),
    ];
    // Content roots are explicit fixture inputs; production must obtain
    // these entries from its publisher, not fabricate them from geometry.
    let mut entries = std::array::from_fn(|i| ManifestEntryV1 {
        subject: subjects[i],
        content_root: subjects[i],
        codec: codecs[i],
        total_bytes: 1,
        chunk_bytes: limits[i].chunk_bytes,
    });
    let scopes = [
        MaterialScope::Artifact,
        MaterialScope::Program,
        MaterialScope::Prompt,
        MaterialScope::InputTokens,
        MaterialScope::Tokenizer,
        MaterialScope::ExecutionRules,
        MaterialScope::InitialState,
        MaterialScope::Dependencies,
        MaterialScope::Trace,
        MaterialScope::Output,
    ];
    for kind in scopes {
        entries[kind as usize].total_bytes = plan.supply.body_bytes_v1(kind);
    }
    entries[MaterialScope::Artifact as usize] = descriptor.manifest_entry_v1(limits[MaterialScope::Artifact as usize]).unwrap();
    entries[MaterialScope::Prompt as usize] =
        prompt_text_material_v1(scope.ruleset, "fixture prompt", limits[MaterialScope::Prompt as usize]).unwrap();
    entries[MaterialScope::ExecutionRules as usize] = super::super::super::tfp1_material::native_ordinary_tfp1_rules_material_v1(
        fp,
        plan.source_v3(),
        &plan.class,
        form,
        limits[MaterialScope::ExecutionRules as usize],
    )
    .unwrap()
    .0;
    let material = plan.unsigned_material_v3(fp, b"fixture prompt", entries).unwrap();
    let book =
        ServiceReservations::new(ServiceCost { bytes: 1_000_000_000, work: 1_000_000_000, io: 1_000_000_000, controls: 65_536 })
            .unwrap();
    let before = book.clone();
    let quote = plan.quote_with_book_v3(fp, b"fixture prompt", &descriptor_bytes, entries, &book, price).unwrap();
    assert_eq!(quote.material.companion, material.companion);
    assert_eq!(quote.material.manifest.as_slice(), material.manifest.as_slice());
    assert_eq!(quote.funding.booking.original_v1(), material.companion.id());
    assert!(quote.funding.original_fee_sompi > policy.funding.original_fee_sompi, "early service and storage must also be funded");
    assert_eq!(book, before);

    // The client must refuse a quote substituted between reading the node and
    // signing either companion or payer consent. This uses the planner's actual
    // unsigned values above, without treating the quote as an accepted origin.
    use crate::palw_protected_original_quote_v1::{ProtectedOriginalQuoteMaterialV1, ProtectedOriginalQuoteV1};
    let literal_material = ProtectedOriginalQuoteMaterialV1 {
        prompt_encoding: misaka_palw_positive_contract::public_one_honest::PromptMaterialEncodingV1::LiteralUtf8,
        prompt: b"fixture prompt".to_vec(),
        hf_descriptor: descriptor_bytes.clone(),
        entries,
    };
    let client_quote = ProtectedOriginalQuoteV1 {
        version: 2,
        parent: parent.last_point().unwrap().block,
        quote_daa: at.daa_score,
        quote_blue_score: at.blue_score,
        claim: crate::palw_freeprompt_v3::fp_claim_id_v3(fp),
        companion: quote.material.companion.clone(),
        manifest: quote.material.manifest.to_vec(),
        funding: quote.funding.funding.clone(),
        original_fee_sompi: quote.funding.original_fee_sompi,
        future_carrier_fee_sompi: quote.funding.future_carrier_fee_sompi,
    };
    let check = |quote: &ProtectedOriginalQuoteV1, material: &ProtectedOriginalQuoteMaterialV1| {
        quote.check_signing_material_v1(fp, header, material, &fp.job.executor_pubkey)
    };
    check(&client_quote, &literal_material).unwrap();
    // Also exercise the node's actual pinned quote policy and its rooted
    // service book. The generous arithmetic policy above cannot demonstrate
    // that the Studio/RPC quote entrance can admit this native geometry.
    // Material content here remains a fixture; this is not a Panel replay.
    // The old profile remains insufficient. Check that refusal before using a
    // separately declared fixture profile for the positive native quote case.
    let pinned_params = params.clone().with_family_supplier_from(Some(at.daa_score));
    let mut pinned_extras = extras.clone();
    pinned_extras.family_supplier_active = true;
    assert!(pinned_extras.tir_public_conviction_active);
    let schedules = super::super::public_policy::ProtectedPlanningSchedulesV3::pinned_v3().unwrap();
    let stored_schedule = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().unwrap();
    // The resolver consumes the complete preimage selected by its caller;
    // it must not reselect material TTL after the other inputs were chosen.
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(Some(
        crate::palw_court_shared_material_intake_v1::MaterialIntakeScheduleV1 { draft_ttl_daa: 0, ..stored_schedule },
    ));
    let pinned = super::super::public_policy::resolve_protected_planning_policy_v3(
        parent,
        &pinned_extras,
        &pinned_params,
        at.daa_score,
        at.blue_score,
        fp,
        header,
        &literal_material.prompt,
        &descriptor_bytes,
        form,
        true,
        literal_material.prompt_encoding,
        schedules,
    );
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(Some(stored_schedule));
    let pinned = pinned.expect("the selected policy resolves for the native registered fixture without re-reading TTL");
    let pinned_policy = pinned.policy();
    // The resolver must cover the actual final native projection, including
    // requests and retained faults, before the unchanged rooted capacity gate.
    let accounting_plan = NativeTfp1OriginalPlanV3::derive_v3(
        parent,
        &pinned_extras,
        &pinned_params,
        at.daa_score,
        at.blue_score,
        fp,
        header,
        form,
        &literal_material.prompt,
        &descriptor_bytes,
        &pinned_policy,
        |stage, bytes| pinned.price_delivery_v1(stage, bytes),
    )
    .unwrap();
    let accounting_terms = accounting_plan
        .projection_v3()
        .compile_prospective_original_terms_v2(
            pinned_policy.service_profile,
            pinned_policy.budget.retention_until,
            pinned_policy.delivery,
            pinned_policy.tokenizer,
            pinned_policy.storage,
            |stage, bytes| pinned.price_delivery_v1(stage, bytes),
        )
        .unwrap();
    let requests = MaterialRequestSeriesV2::new_v2(
        InitialMaterialRequestPlanV1::new_v1(
            at.daa_score,
            pinned_policy.notice,
            pinned_policy.request_service,
            pinned_policy.request_retained_bytes,
        )
        .unwrap(),
        pinned_policy.request_max_gap_daa,
    )
    .unwrap();
    let accounting = accounting_terms
        .accounting_with_requests_v2(pinned_policy.request_service, pinned_policy.request_retained_bytes, requests.count_v2())
        .unwrap();
    let defensive = pinned_policy.rescue.defensive;
    assert!(defensive.transmitted_bytes >= accounting.service.bytes);
    assert_eq!(defensive.additional_work, accounting.service.work + pinned_policy.initial_supply.additional_work);
    assert!(defensive.retained_bytes >= accounting.retained_payload_bytes + accounting.retained_numerical_fault_bytes);
    let total = pinned_policy
        .rescue
        .per_drawn_bond
        .checked_scale(pinned_policy.rescue.max_drawn_bonds)
        .unwrap()
        .checked_add(defensive)
        .unwrap()
        .checked_add(pinned_policy.initial_supply)
        .unwrap();
    assert_eq!(total.transmitted_bytes, pinned_policy.budget.transmitted_bytes);
    assert_eq!(total.additional_work, pinned_policy.budget.additional_work);
    assert_eq!(total.retained_bytes, pinned_policy.budget.retained_bytes);
    assert!(pinned_policy.budget.fee >= accounting.storage_rent_sompi * (pinned_policy.rescue.max_drawn_bonds + 1));
    eprintln!(
        "protected native accounting: requests_per_case={} initial={:?} defensive={:?} drawn={:?} aggregate={:?} fee_ceiling={}",
        requests.count_v2(),
        pinned_policy.initial_supply,
        defensive,
        pinned_policy.rescue.per_drawn_bond,
        total,
        pinned_policy.budget.fee
    );
    drop(accounting_plan);
    let numerical = pinned_policy.delivery.numerical_verification.service_cost_v1().unwrap();
    let terminal_base = ServiceCost {
        work: numerical.work + pinned_policy.tokenizer.verification.work,
        io: numerical.io + pinned_policy.tokenizer.verification.io,
        ..numerical
    };
    pinned_policy.selection.check_terminal_v3(terminal_base, pinned_policy.delivery.terminal).unwrap();
    let old_terminal = crate::palw_g14_values_v1::palw_g14_funded_batch_schedule_estimate_v1().delivery.terminal;
    assert!(pinned_policy.selection.check_terminal_v3(terminal_base, old_terminal).is_err());
    let underfunded = ServiceCost { work: pinned_policy.delivery.terminal.work - 1, ..pinned_policy.delivery.terminal };
    assert!(pinned_policy.selection.check_terminal_v3(terminal_base, underfunded).is_err());
    let pinned_limits = pinned_policy.admission.manifest_scope_limits;
    let mut pinned_material = literal_material.clone();
    for (entry, limit) in pinned_material.entries.iter_mut().zip(pinned_limits) {
        entry.chunk_bytes = limit.chunk_bytes;
    }
    pinned_material.entries[MaterialScope::Artifact as usize] =
        descriptor.manifest_entry_v1(pinned_limits[MaterialScope::Artifact as usize]).unwrap();
    pinned_material.entries[MaterialScope::Prompt as usize] = prompt_text_material_v1(
        pinned_policy.admission.scope.ruleset,
        "fixture prompt",
        pinned_limits[MaterialScope::Prompt as usize],
    )
    .unwrap();
    let cold = super::super::super::tfp1_material::public_tfp1_cold_material_v1(
        fp,
        plan.source_v3(),
        &plan.class,
        pinned_limits[MaterialScope::InitialState as usize],
        pinned_limits[MaterialScope::Dependencies as usize],
    )
    .unwrap();
    pinned_material.entries[MaterialScope::InitialState as usize] = cold.initial_state.0;
    pinned_material.entries[MaterialScope::Dependencies as usize] = cold.dependencies.0;
    pinned_material.entries[MaterialScope::ExecutionRules as usize] =
        super::super::super::tfp1_material::native_ordinary_tfp1_rules_material_v1(
            fp,
            plan.source_v3(),
            &plan.class,
            form,
            pinned_limits[MaterialScope::ExecutionRules as usize],
        )
        .unwrap()
        .0;
    let parent_root = parent.state_root();
    let refused = crate::palw_protected_original_quote_v1::quote_protected_original_v1(
        parent,
        &pinned_params,
        &pinned_extras,
        at.daa_score,
        at.blue_score,
        fp,
        header,
        &pinned_material,
    )
    .expect_err("the unchanged pinned work/IO budget must refuse this native original");
    assert!(refused.contains("ReservationCapacity") && refused.contains("AtomicPacketExceedsBudget"), "{refused}");
    assert_eq!(parent.state_root(), parent_root);
    let mut private_extras = extras.clone();
    let declaration = private_fixture_declaration_v3(policy.cover);
    private_extras.protected_planning_v3 = Some(declaration);
    // No legacy family-supplier bootstrap is borrowed by the private owner.
    private_extras.family_supplier_active = false;
    let private = super::super::public_policy::resolve_protected_planning_policy_v3(
        parent,
        &private_extras,
        params,
        at.daa_score,
        at.blue_score,
        fp,
        header,
        &literal_material.prompt,
        &descriptor_bytes,
        form,
        true,
        literal_material.prompt_encoding,
        super::super::public_policy::ProtectedPlanningSchedulesV3::selected_v3(Some(declaration)).unwrap(),
    )
    .unwrap();
    let private_policy = private.policy();
    // Exercise the same explicit policy binding now used by the publisher,
    // against the actual native resolver and complete quote below.
    let private_material = pinned_material.clone().with_planning_declaration_v3(&declaration).unwrap();
    assert_eq!(private_material.prompt, pinned_material.prompt);
    assert_eq!(private_material.hf_descriptor, pinned_material.hf_descriptor);
    let expected_prompt = prompt_text_material_v1(
        private_policy.admission.scope.ruleset,
        "fixture prompt",
        private_policy.admission.manifest_scope_limits[MaterialScope::Prompt as usize],
    )
    .unwrap();
    assert_eq!(private_material.entries[MaterialScope::Prompt as usize], expected_prompt);
    for index in 0..MaterialScope::COUNT {
        if index != MaterialScope::Prompt as usize {
            assert_eq!(private_material.entries[index], pinned_material.entries[index]);
        }
    }
    let private_quote = crate::palw_protected_original_quote_v1::quote_protected_original_v1(
        parent,
        params,
        &private_extras,
        at.daa_score,
        at.blue_score,
        fp,
        header,
        &private_material,
    )
    .expect("the declared fixture capacity must pass the complete native quote and all schedule/funding constraints");
    check(&private_quote, &private_material).unwrap();
    assert_eq!(private_quote.claim, client_quote.claim);
    assert_eq!(private_quote.parent, parent.last_point().unwrap().block);
    assert_eq!(parent.state_root(), parent_root, "a quote does not install an admission or reserve funds");
    assert_eq!(client_quote.funding_window_v2(fp).unwrap(), (at.daa_score, at.daa_score + 64, fp.trace_retention_daa));
    let mut short_lived = fp.clone();
    short_lived.trace_retention_daa = at.daa_score + 11_999;
    assert!(client_quote.funding_window_v2(&short_lived).is_err());
    let mut short_quote = client_quote.clone();
    short_quote.funding.required_material_until_daa = at.daa_score + 9_999;
    assert!(short_quote.funding_window_v2(fp).is_err());
    let mut substituted = client_quote.clone();
    substituted.funding.claim = Hash64::from_u64_word(998);
    assert!(check(&substituted, &literal_material).is_err());
    substituted = client_quote.clone();
    substituted.funding.execution_root = Hash64::from_u64_word(998);
    assert!(check(&substituted, &literal_material).is_err());
    substituted = client_quote.clone();
    substituted.companion.program = Digest([99; 64]);
    assert!(check(&substituted, &literal_material).is_err());
    let mut changed_material = literal_material.clone();
    changed_material.prompt[0] ^= 1;
    assert!(check(&client_quote, &changed_material).is_err());
    changed_material = literal_material.clone();
    changed_material.hf_descriptor[0] ^= 1;
    assert!(check(&client_quote, &changed_material).is_err());
    changed_material = literal_material.clone();
    changed_material.entries[MaterialScope::Trace as usize].content_root = Digest([99; 64]);
    assert!(check(&client_quote, &changed_material).is_err());
    assert!(client_quote.check_signing_material_v1(fp, header, &literal_material, &[0; 2592]).is_err());

    // Explicit segmented policy preserves the same native FP claim while
    // binding a distinct public prompt scope and manifest. No origin is installed.
    use misaka_palw_positive_contract::public_one_honest::PromptMaterialEncodingV1;
    let encoding = PromptMaterialEncodingV1::SegmentsV1;
    let segmented_prompt = "PPS1\ns:128\nt:14:fixture prompt";
    let mut segmented_limits = limits;
    segmented_limits[MaterialScope::Prompt as usize].codec = encoding.codec_v1();
    let mut segmented_policy = policy.clone();
    segmented_policy.admission.manifest_scope_limits = &segmented_limits;
    segmented_policy.admission.scope.ruleset = encoding.ruleset_v1(scope.ruleset);
    let segmented_plan = NativeTfp1OriginalPlanV3::derive_v3(
        parent,
        extras,
        params,
        at.daa_score,
        at.blue_score,
        fp,
        header,
        form,
        segmented_prompt.as_bytes(),
        &descriptor_bytes,
        &segmented_policy,
        price,
    )
    .unwrap();
    let mut segmented_entries = entries;
    segmented_entries[MaterialScope::Prompt as usize] = encoding
        .material_v1(segmented_policy.admission.scope.ruleset, segmented_prompt, segmented_limits[MaterialScope::Prompt as usize])
        .unwrap();
    let segmented_quote = segmented_plan
        .quote_with_book_v3(fp, segmented_prompt.as_bytes(), &descriptor_bytes, segmented_entries, &book, price)
        .unwrap();
    assert_ne!(segmented_quote.material.companion.id(), quote.material.companion.id());
    let segmented_material = ProtectedOriginalQuoteMaterialV1 {
        prompt_encoding: encoding,
        prompt: segmented_prompt.as_bytes().to_vec(),
        hf_descriptor: descriptor_bytes.clone(),
        entries: segmented_entries,
    };
    let mut segmented_client = ProtectedOriginalQuoteV1 {
        version: 3,
        companion: segmented_quote.material.companion.clone(),
        manifest: segmented_quote.material.manifest.to_vec(),
        funding: segmented_quote.funding.funding.clone(),
        original_fee_sompi: segmented_quote.funding.original_fee_sompi,
        future_carrier_fee_sompi: segmented_quote.funding.future_carrier_fee_sompi,
        ..client_quote.clone()
    };
    check(&segmented_client, &segmented_material).unwrap();
    assert_eq!(segmented_client.claim, client_quote.claim);
    segmented_client.version = 2;
    assert!(check(&segmented_client, &segmented_material).is_err());
    assert!(plan.unsigned_material_v3(fp, segmented_prompt.as_bytes(), segmented_entries).is_err());
    assert_eq!(book, before);

    // One pre-signing original remains identical after delayed DAA inclusion.
    // Physical deadlines are recompiled at the new acceptance clock. This
    // arithmetic fixture holds the authenticated Selection blue snapshot fixed.
    let shift = 7;
    let mut later_policy = policy.clone();
    later_policy.notice.draw_ready_by += shift;
    later_policy.notice.material_ready_by += shift;
    later_policy.notice.missing_material_detected_by += shift;
    later_policy.budget.retention_until += shift;
    let later = NativeTfp1OriginalPlanV3::derive_v3(
        parent,
        extras,
        params,
        at.daa_score + shift,
        at.blue_score,
        fp,
        header,
        form,
        b"fixture prompt",
        &descriptor_bytes,
        &later_policy,
        price,
    )
    .unwrap();
    let later_quote = later.quote_with_book_v3(fp, b"fixture prompt", &descriptor_bytes, entries, &book, price).unwrap();
    assert_eq!(later_quote.material.companion, quote.material.companion, "delayed inclusion cannot change the signed original");
    assert_eq!(later_quote.material.manifest.as_slice(), quote.material.manifest.as_slice());
    assert_eq!(later_quote.funding.funding.relative_graph_id, quote.funding.funding.relative_graph_id);
    assert_eq!(later_quote.funding.funding.required_amount_sompi, quote.funding.funding.required_amount_sompi);
    assert_eq!(later_quote.funding.funding.required_material_until_daa, quote.funding.funding.required_material_until_daa + shift);
    later
        .admission_v3(fp, b"fixture prompt")
        .unwrap()
        .check_tfp1_candidate_v1(&quote.material.companion, later.terms.obligation_v1(), &later.prospective_v3())
        .unwrap();

    // V1 keeps its absolute budget identity and cannot be reinterpreted as V2.
    let mut legacy_policy = policy.clone();
    legacy_policy.relative_service_v2 = false;
    let legacy = NativeTfp1OriginalPlanV3::derive_v3(
        parent,
        extras,
        params,
        at.daa_score,
        at.blue_score,
        fp,
        header,
        form,
        b"fixture prompt",
        &descriptor_bytes,
        &legacy_policy,
        price,
    )
    .unwrap();
    let legacy_material = legacy.unsigned_material_v3(fp, b"fixture prompt", entries).unwrap();
    assert_ne!(legacy_material.companion.service_contract, quote.material.companion.service_contract);
    assert!(
        later
            .admission_v3(fp, b"fixture prompt")
            .unwrap()
            .check_tfp1_candidate_v1(&legacy_material.companion, later.terms.obligation_v1(), &later.prospective_v3())
            .is_err()
    );

    // A fresh signed-intake derivation has the same relative obligation and
    // metadata. It receives an actual origin separately, never from a quote.
    let signed_plan = derive(&descriptor_bytes, at.blue_score).unwrap();
    assert_eq!(signed_plan.terms.obligation_v1(), plan.terms.obligation_v1());
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64(at.block.as_bytes()),
        carrier_block: NativeHash64([97; 64]),
        carrier_transaction: NativeHash64([98; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: 1,
        accepted_daa: at.daa_score,
        accepted_blue_score: at.blue_score,
    };
    assert_eq!(signed_plan.actual_v3(origin).unwrap().service_obligation, plan.prospective_v3().service_obligation);
    assert!(signed_plan.actual_v3(AcceptedOrigin { accepted_blue_score: at.blue_score + 1, ..origin }).is_err());
    assert!(derive(&descriptor_bytes, parent.last_point().unwrap().blue_score).is_err());
    let mut changed = entries;
    changed[MaterialScope::Trace as usize].total_bytes += 1;
    assert!(plan.quote_with_book_v3(fp, b"fixture prompt", &descriptor_bytes, changed, &book, price).is_err());
    changed = entries;
    changed[MaterialScope::Artifact as usize].content_root = Digest([99; 64]);
    assert!(plan.unsigned_material_v3(fp, b"fixture prompt", changed).is_err());
    for kind in scopes {
        let mut changed = entries;
        changed[kind as usize].content_root = Digest([99; 64]);
        assert!(plan.unsigned_material_v3(fp, b"fixture prompt", changed).is_err(), "changed {kind:?} content root accepted");
    }
    assert!(plan.unsigned_material_v3(fp, b"Fixture prompt", entries).is_err());
    assert_eq!(book, before);
}
