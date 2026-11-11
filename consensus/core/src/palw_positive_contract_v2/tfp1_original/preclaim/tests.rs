use super::*;
use crate::{palw_state_v2::PalwBlockContextV2, palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1};
use misaka_palw_positive_contract::{
    identity::Digest,
    public_one_honest::{FirstNoticeDemand, MaterialCaseDemand, OriginalClock, VerificationSchedule},
    reservations::RemainingCapacity,
};
use std::collections::BTreeSet;

pub(crate) fn exercise_preclaim_booking_v1(
    projection: &PlainBatchOriginalProjectionV2,
    projected: PlainBatchProspectiveOriginalTermsV2,
    fixture: &PalwTirPublicFrameFixtureV1,
    at: &PalwBlockContextV2,
    late_selection: bool,
) {
    assert!(projected.matches_acceptance_v2(at.daa_score, at.blue_score));
    assert_eq!(projected.obligation_v2().facts_v1().native_identity_v1().0, fixture.frame.plan_v1().original_claim);
    // First exercise the common physical booking kernel with a local namespace.
    // The real signed companion is exercised below; neither is accepted draw authority.
    let original = OriginalId(Digest([83; 64]));
    let allowance = RemainingCapacity { transmitted_bytes: 100_000_000, additional_work: 100_000_000, retained_bytes: 100_000_000 };
    let rescue = ProtectedRescueDemand {
        max_drawn_bonds: 2,
        per_drawn_bond: allowance,
        max_outsiders: 0,
        per_outsider: RemainingCapacity { transmitted_bytes: 0, additional_work: 0, retained_bytes: 0 },
        defensive: allowance,
    };
    let terms = DrawnBatchPreclaimTermsV1::new_v1(projected, rescue).unwrap();
    let notice = FirstNoticeDemand {
        draw_ready_by: at.daa_score + 1,
        material_ready_by: at.daa_score + 1,
        cold_acquisition_daa: 1,
        full_inspection_daa: 1,
        notice_inclusion_daa: 1,
        missing_material_detected_by: at.daa_score + 1,
        material_request_inclusion_daa: 1,
    };
    let case = MaterialCaseDemand {
        material_delivery_daa: 1_000,
        full_inspection_daa: 1_000,
        localization_daa: 1_000,
        filing_daa: 1_000,
        adjudication_and_recovery_daa: 1_000,
    };
    let clock = OriginalClock::new(at.daa_score).unwrap();
    let schedule = VerificationSchedule::new(clock, notice, case).unwrap();
    let short = VerificationSchedule::new(clock, notice, MaterialCaseDemand { material_delivery_daa: 1, ..case }).unwrap();
    let price = |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
    let empty_book =
        ServiceReservations::new(ServiceCost { bytes: 40_000_000, work: 20_000_000, io: 20_000_000, controls: 512 }).unwrap();
    let scope = |rank| drawn_scope_v1(terms.obligation_v1(), original, rank, rescue.max_drawn_bonds).unwrap();
    let mut case_ids = BTreeSet::new();
    let mut width = 0;
    for rank in 0..rescue.max_drawn_bonds {
        let mut book = empty_book.clone();
        let (quote, graph) = projection.reserve_before_allocation_v2(&mut book, scope(rank), &projected, price).unwrap();
        assert_eq!(quote.scope, scope(rank));
        assert!(graph.iter().all(|node| case_ids.insert(node.packet.id)), "the ranks must not share physical packets");
        width = graph.len();
    }
    let maximum_packets = width.checked_mul(rescue.max_drawn_bonds as usize).unwrap();
    let mut booked = empty_book.clone();
    let (quotes, graph) = reserve_drawn_v1(original, projection, &terms, schedule, &mut booked, maximum_packets, price).unwrap();
    assert_ne!(scope(0), scope(1));
    assert_eq!(quotes.len(), 2);
    assert_eq!(graph.len(), maximum_packets);
    assert_eq!(graph.iter().map(|node| node.packet.id).collect::<BTreeSet<_>>(), case_ids);
    assert!(case_ids.iter().all(|id| booked.assigned_daa(id).is_some()));
    for (rank, quote) in quotes.iter().enumerate() {
        assert_eq!(quote.scope, scope(rank as u64));
        assert_eq!(quote.relative_graph_id, projected.relative_graph_id_v2());
    }

    let mut unchanged = empty_book.clone();
    assert!(matches!(
        reserve_drawn_v1(original, projection, &terms, schedule, &mut unchanged, maximum_packets - 1, price),
        Err(BatchCourtErrorV1::CaseGraph)
    ));
    assert_eq!(unchanged, empty_book);

    let underfunded = DrawnBatchPreclaimTermsV1::new_v1(
        projected,
        ProtectedRescueDemand { per_drawn_bond: RemainingCapacity { additional_work: 1, ..allowance }, ..rescue },
    )
    .unwrap();
    assert_ne!(underfunded.obligation_v1(), terms.obligation_v1());
    assert!(matches!(
        reserve_drawn_v1(original, projection, &underfunded, schedule, &mut unchanged, maximum_packets, price),
        Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
    ));
    assert_eq!(unchanged, empty_book);

    assert!(reserve_drawn_v1(original, projection, &terms, short, &mut unchanged, maximum_packets, price).is_err());
    assert_eq!(unchanged, empty_book);

    if late_selection {
        // The native schedule, not an early fixture's knowledge of the row,
        // decides when the target-binding opportunity can begin. Moving
        // localization later must refuse the whole protected reservation.
        for changed in [
            MaterialCaseDemand { localization_daa: case.localization_daa + 1_000, ..case },
            MaterialCaseDemand { filing_daa: 1, ..case },
            // test_only_v3 ends delivery at localized_by + 13; the
            // signed Verify carrier files one DAA later. Completing the
            // ranges alone must not count as timely evidence filing.
            MaterialCaseDemand { filing_daa: 13, ..case },
        ] {
            let schedule = VerificationSchedule::new(clock, notice, changed).unwrap();
            assert!(matches!(
                reserve_drawn_v1(original, projection, &terms, schedule, &mut unchanged, maximum_packets, price),
                Err(BatchCourtErrorV1::Reservation(ServiceRefusal::DeadlineExhausted))
            ));
            assert_eq!(unchanged, empty_book, "late binding/delivery refusal cannot leave a sibling rank reserved");
        }
    }

    check_companion_booking_parity(projection, &terms, fixture, at, notice, case, &empty_book, maximum_packets);

    let bytes = super::super::manifest_carrier::MANIFEST_REQUEST_CARRIER_BYTES_V1 as u64;
    let retained = misaka_palw_positive_contract::public_one_honest::PendingManifestCaseRecordV1::WIRE_BYTES as u64;
    let initial_cost = ServiceCost { bytes, work: bytes, io: retained, controls: 1 };
    let initial_plan = InitialMaterialRequestPlanV1::new_v1(at.daa_score, notice, initial_cost, retained).unwrap();
    let with_initial = terms.with_initial_request_v2(initial_plan).unwrap();
    assert_ne!(with_initial.obligation_v1(), terms.obligation_v1());
    assert_ne!(with_initial.relative_graph, terms.relative_graph);
    let complete_cap = maximum_packets + rescue.max_drawn_bonds as usize;
    let mut complete_book = empty_book.clone();
    let (complete_quotes, complete_graph) =
        reserve_drawn_v1(original, projection, &with_initial, schedule, &mut complete_book, complete_cap, price).unwrap();
    assert_eq!(complete_graph.len(), complete_cap);
    for (rank, quote) in complete_quotes.iter().enumerate() {
        let scope = drawn_scope_v1(with_initial.obligation, original, rank as u64, rescue.max_drawn_bonds).unwrap();
        let request = initial_plan.request_v1(quote.claim, scope, at.daa_score).unwrap();
        assert_eq!(complete_book.assigned_daa(&request.packet.id), Some(schedule.first_material_request_by()));
        assert_eq!(quote.relative_graph_id, with_initial.relative_graph);
        assert_eq!(quote.service.bytes, quotes[rank].service.bytes + initial_cost.bytes);
        assert_eq!(quote.service.work, quotes[rank].service.work + initial_cost.work);
        assert_eq!(quote.service.controls, quotes[rank].service.controls + 1);
        assert_eq!(quote.retained_payload_bytes, quotes[rank].retained_payload_bytes + retained);
        assert!(quote.carrier_storage_rent_sompi > quotes[rank].carrier_storage_rent_sompi);
        assert!(complete_graph.iter().any(|node| node.predecessors.contains(&request.packet.id)));
    }
    let mut refused = empty_book.clone();
    assert!(reserve_drawn_v1(original, projection, &with_initial, schedule, &mut refused, complete_cap - 1, price).is_err());
    assert_eq!(refused, empty_book);
    check_companion_booking_parity(projection, &with_initial, fixture, at, notice, case, &empty_book, complete_cap);

    // A wider request interval permits two different readiness floors with
    // the same deadline. Signed and unsigned paths must bind both ends.
    let wide_notice = FirstNoticeDemand { material_request_inclusion_daa: 2, ..notice };
    let wide_plan = InitialMaterialRequestPlanV1::new_v1(at.daa_score, wide_notice, initial_cost, retained).unwrap();
    let wide_terms = terms.with_initial_request_v2(wide_plan).unwrap();
    check_companion_booking_parity(projection, &wide_terms, fixture, at, wide_notice, case, &empty_book, complete_cap);
}

#[allow(clippy::too_many_arguments)]
fn check_companion_booking_parity(
    projection: &PlainBatchOriginalProjectionV2,
    terms: &DrawnBatchPreclaimTermsV1,
    fixture: &PalwTirPublicFrameFixtureV1,
    at: &PalwBlockContextV2,
    notice: FirstNoticeDemand,
    case: MaterialCaseDemand,
    empty_book: &ServiceReservations,
    maximum_packets: usize,
) {
    use misaka_palw_positive_contract::{
        identity::{ChainScope, principal},
        messages::AggregateBudget,
        public_one_honest::{
            ManifestScopeRules, ManifestV1, MaterialScope, ORIGINAL_SIGN_CONTEXT, SignedPublicOriginalV1,
            prompt_text_material_codec_v1, prompt_text_material_v1, tir_cold_dependencies_codec_v1,
            tir_cold_dependencies_for_execution_v1, tir_cold_initial_state_codec_v1, tir_cold_initial_state_for_execution_v1,
        },
        rows::{AcceptedOrigin, AggregateUsage, NativeHash64},
    };
    // This key is chosen by the native tokenizer fixture before execution.
    // The test never changes the committed job or its Source.
    let signer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
    let key = *signer.verification_key.as_ref();
    let frame = &fixture.frame;
    let fp = frame.original_v1();
    assert_eq!(fp.job.executor_pubkey.as_slice(), key.as_slice());
    let binding = frame
        .header_v1()
        .resolve_registered_layout_v1(&fixture.record, &fixture.layout, fp.execution_root, fixture.rules.maximum_leaves)
        .unwrap();
    let job = &binding.job_context;
    let d = Digest([85; 64]);
    let scope = ChainScope { network: d, ruleset: d, branch: d };
    let mut limits = [ManifestScopeRules { codec: d, max_bytes: 1, chunk_bytes: 1 }; MaterialScope::COUNT];
    let prompt_rules = ManifestScopeRules { codec: prompt_text_material_codec_v1(), max_bytes: 64, chunk_bytes: 64 };
    let initial_rules = ManifestScopeRules { codec: tir_cold_initial_state_codec_v1(), max_bytes: 128, chunk_bytes: 128 };
    let dependency_rules = ManifestScopeRules { codec: tir_cold_dependencies_codec_v1(), max_bytes: 68, chunk_bytes: 68 };
    limits[MaterialScope::Prompt as usize] = prompt_rules;
    limits[MaterialScope::InitialState as usize] = initial_rules;
    limits[MaterialScope::Dependencies as usize] = dependency_rules;
    let digest = |h: Hash64| Digest(h.as_bytes());
    let execution = digest(job.context_hash());
    let program = digest(binding.class.graph_ir_root());
    let initial_state = tir_cold_initial_state_for_execution_v1(program, execution, initial_rules).unwrap().0.subject;
    let dependencies = tir_cold_dependencies_for_execution_v1(execution, dependency_rules).unwrap().0.subject;
    let prompt = prompt_text_material_v1(scope.ruleset, "fixture prompt", prompt_rules).unwrap().subject;
    let initial_supply = terms.rescue.per_drawn_bond;
    let capacity = initial_supply.checked_add(terms.rescue.total().unwrap()).unwrap();
    let budget = AggregateBudget {
        rounds: 1,
        unique_source_bytes: 100_000_000,
        transmitted_bytes: capacity.transmitted_bytes,
        additional_work: capacity.additional_work,
        retained_bytes: capacity.retained_bytes,
        retention_until: terms.projected.material_until_daa_v2(),
        fee: 1,
        witness_attempts: 1,
    };
    let obligation = terms.obligation_v1();
    let service_contract = obligation.service_contract_v1(&budget);
    let admission = PublicOriginalAdmissionV1 {
        scope,
        job,
        class: &binding.class,
        artifact: binding.artifact_root,
        miner: principal(&key),
        prompt,
        initial_state,
        dependencies,
        service_contract,
        max_manifest_bytes: ManifestV1::WIRE_BYTES,
        manifest_scope_limits: &limits,
        manifest_total_limit: 64 + 128 + 68 + MaterialScope::COUNT as u64 - 3,
    };
    let usage = AggregateUsage::default();
    let reservation = PublicOriginalReservationV1 {
        origin: AcceptedOrigin {
            accepting_block: NativeHash64(at.block.as_bytes()),
            carrier_block: NativeHash64([86; 64]),
            carrier_transaction: NativeHash64([87; 64]),
            accepted_tx_ordinal: 0,
            event_ordinal: 0,
            accepted_daa: at.daa_score,
            accepted_blue_score: at.blue_score,
        },
        service_obligation: obligation.obligation_v1(),
        notice,
        case,
        initial_supply,
        rescue: terms.rescue,
        usage: &usage,
        budget: &budget,
    };
    let companion = OriginalCommit {
        scope,
        job: digest(job.job_id),
        execution,
        miner: principal(&key),
        registered_class: digest(job.shape_profile_id),
        artifact: digest(binding.artifact_root),
        program,
        prompt,
        input_tokens: digest(job.prompt_token_ids_hash),
        tokenizer: digest(job.tokenizer_id),
        execution_rules: digest(job.cu_ruleset_id),
        generation_seed: job.execution_seed,
        initial_state,
        dependencies,
        prefill: job.declared_prefill_tokens,
        decode: job.exact_decode_tokens,
        max_context: job.max_context_tokens,
        output: digest(fp.output_root),
        trace_root: digest(fp.execution_root),
        manifest_root: d,
        service_contract,
    };
    let sign_companion = |original: &OriginalCommit| {
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&signer.signing_key, &original.id().0.0, ORIGINAL_SIGN_CONTEXT, [88; 32]).unwrap();
        borsh::to_vec(&SignedPublicOriginalV1 { original: original.clone(), key, signature: *signature.as_ref() }).unwrap()
    };
    let signed = sign_companion(&companion);
    let prepared = admission.prepare_tfp1_signed_bytes_v1(&signed, obligation, &reservation).unwrap();
    let price = |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
    let mut unsigned_book = empty_book.clone();
    let (unsigned_quotes, unsigned_graph) = admission
        .reserve_tfp1_drawn_batch_candidate_v1(&companion, &reservation, projection, terms, &mut unsigned_book, maximum_packets, price)
        .unwrap();
    let mut signed_book = empty_book.clone();
    let (signed_quotes, signed_graph) =
        prepared.reserve_drawn_batch_candidates_v1(projection, terms, &mut signed_book, maximum_packets, price).unwrap();
    assert_eq!(unsigned_quotes, signed_quotes);
    assert_eq!(unsigned_graph, signed_graph);
    assert_eq!(unsigned_book, signed_book);

    let mut unchanged = empty_book.clone();
    if terms.initial_request.is_some() && notice.material_request_inclusion_daa > 1 {
        let later_notice = FirstNoticeDemand {
            missing_material_detected_by: notice.missing_material_detected_by + 1,
            material_request_inclusion_daa: notice.material_request_inclusion_daa - 1,
            ..notice
        };
        let clock = OriginalClock::new(at.daa_score).unwrap();
        assert_eq!(
            VerificationSchedule::new(clock, notice, case).unwrap(),
            VerificationSchedule::new(clock, later_notice, case).unwrap(),
            "a deadline-only comparison cannot distinguish these reservations",
        );
        let later_reservation = PublicOriginalReservationV1 { notice: later_notice, ..reservation };
        let later_prepared = admission.prepare_tfp1_signed_bytes_v1(&signed, obligation, &later_reservation).unwrap();
        assert!(matches!(
            admission.reserve_tfp1_drawn_batch_candidate_v1(
                &companion,
                &later_reservation,
                projection,
                terms,
                &mut unchanged,
                maximum_packets,
                price,
            ),
            Err(BatchCourtErrorV1::CaseGraph)
        ));
        assert_eq!(&unchanged, empty_book);
        assert!(matches!(
            later_prepared.reserve_drawn_batch_candidates_v1(projection, terms, &mut unchanged, maximum_packets, price),
            Err(BatchCourtErrorV1::Window)
        ));
        assert_eq!(&unchanged, empty_book);
    }
    let wrong_blue = PublicOriginalReservationV1 {
        origin: AcceptedOrigin { accepted_blue_score: at.blue_score + 1, ..reservation.origin },
        ..reservation
    };
    assert!(matches!(
        admission.reserve_tfp1_drawn_batch_candidate_v1(
            &companion,
            &wrong_blue,
            projection,
            terms,
            &mut unchanged,
            maximum_packets,
            price,
        ),
        Err(BatchCourtErrorV1::CaseGraph)
    ));
    assert_eq!(&unchanged, empty_book);

    let short_budget = AggregateBudget { retention_until: budget.retention_until - 1, ..budget };
    assert!(short_budget.retention_until > OriginalClock::new(at.daa_score).unwrap().hard());
    let service_contract = obligation.service_contract_v1(&short_budget);
    let short_admission = PublicOriginalAdmissionV1 { service_contract, ..admission };
    let short_companion = OriginalCommit { service_contract, ..companion };
    let short_reservation = PublicOriginalReservationV1 { budget: &short_budget, ..reservation };
    let short_signed = sign_companion(&short_companion);
    // The shortened horizon still covers the public hard clock and the
    // companion is correctly signed. Booking must reject its narrower
    // retention against the already compiled material horizon.
    let short_prepared = short_admission.prepare_tfp1_signed_bytes_v1(&short_signed, obligation, &short_reservation).unwrap();
    assert!(matches!(
        short_admission.reserve_tfp1_drawn_batch_candidate_v1(
            &short_companion,
            &short_reservation,
            projection,
            terms,
            &mut unchanged,
            maximum_packets,
            price,
        ),
        Err(BatchCourtErrorV1::CaseGraph)
    ));
    assert_eq!(&unchanged, empty_book);
    assert!(matches!(
        short_prepared.reserve_drawn_batch_candidates_v1(projection, terms, &mut unchanged, maximum_packets, price),
        Err(BatchCourtErrorV1::CaseGraph)
    ));
    assert_eq!(&unchanged, empty_book);
}
