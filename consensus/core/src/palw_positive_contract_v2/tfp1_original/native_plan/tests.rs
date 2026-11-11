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
    let fp = fixture.frame.original_v1();
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
    let allowance =
        RemainingCapacity { transmitted_bytes: 1_000_000_000, retained_bytes: 1_000_000_000, additional_work: 1_000_000_000 };
    let rescue = ProtectedRescueDemand {
        max_drawn_bonds: 2,
        per_drawn_bond: allowance,
        max_outsiders: 1,
        per_outsider: allowance,
        defensive: allowance,
    };
    let total = allowance.checked_add(rescue.total().unwrap()).unwrap();
    let scope = ChainScope { network: Digest(fp.job.network_domain.as_bytes()), ruleset: Digest([91; 64]), branch: Digest([92; 64]) };
    let request_retained = InitialMaterialRequestPlanV1::minimum_retained_bytes_v1();
    let notice_bytes = crate::palw_court_shared_material_intake_v1::batch::selection::notice::SignedSelectionNoticeV3::BYTES;
    let policy = Tfp1NativePlanningPolicyV3 {
        admission: Tfp1PublicAdmissionPolicyV1 {
            scope,
            max_manifest_bytes: ManifestV1::WIRE_BYTES,
            manifest_scope_limits: &limits,
            manifest_total_limit: 1_000_000_000,
        },
        maximum_leaves: fixture.rules.maximum_leaves,
        cover: StreamingParameterCoverBudgetV1 {
            total: storage.cover,
            per_root_edges: storage.cover.max_total_edges,
            per_root_coordinates: storage.cover.max_total_coordinates,
            per_root_memberships: storage.cover.max_root_memberships,
            max_workspace_bytes: 128 << 20,
        },
        maximum_ranges: 512,
        output_verification: tokenizer.verification,
        output_verification_daa: 1,
        selection,
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
        initial_supply: allowance,
        rescue,
        budget: AggregateBudget {
            rounds: 100,
            unique_source_bytes: 1_000_000_000,
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
    assert_eq!(quote.funding.original_fee_sompi, policy.funding.original_fee_sompi);
    assert_eq!(book, before);

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
