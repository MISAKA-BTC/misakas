use super::*;
use crate::{
    palw_positive_contract_v2::PublicOriginalReservationV1, palw_state_v2::PalwBlockContextV2,
    palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1,
};
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
    let (quotes, graph) =
        reserve_drawn_v1(original, projection, &terms, schedule, u64::MAX, &mut booked, maximum_packets, price).unwrap();
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
        reserve_drawn_v1(original, projection, &terms, schedule, u64::MAX, &mut unchanged, maximum_packets - 1, price),
        Err(BatchCourtErrorV1::CaseGraph)
    ));
    assert_eq!(unchanged, empty_book);

    let trace_bytes = projection.public_trace_payload_bytes_v1().unwrap();
    assert!(trace_bytes > crate::palw_tir_court_v1::PublicOriginalTraceIndexV1::WIRE_BYTES as u64);
    for capacity in [
        RemainingCapacity { transmitted_bytes: trace_bytes - 1, ..allowance },
        RemainingCapacity { retained_bytes: trace_bytes - 1, ..allowance },
    ] {
        let understated =
            DrawnBatchPreclaimTermsV1::new_v1(projected, ProtectedRescueDemand { per_drawn_bond: capacity, ..rescue }).unwrap();
        assert!(matches!(
            reserve_drawn_v1(original, projection, &understated, schedule, u64::MAX, &mut unchanged, maximum_packets, |_, _| {
                panic!("an allowance smaller than native Trace reached graph compilation")
            }),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(unchanged, empty_book);
    }

    let underfunded = DrawnBatchPreclaimTermsV1::new_v1(
        projected,
        ProtectedRescueDemand { per_drawn_bond: RemainingCapacity { additional_work: 1, ..allowance }, ..rescue },
    )
    .unwrap();
    assert_ne!(underfunded.obligation_v1(), terms.obligation_v1());
    assert!(matches!(
        reserve_drawn_v1(original, projection, &underfunded, schedule, u64::MAX, &mut unchanged, maximum_packets, price),
        Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
    ));
    assert_eq!(unchanged, empty_book);

    assert!(reserve_drawn_v1(original, projection, &terms, short, u64::MAX, &mut unchanged, maximum_packets, price).is_err());
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
                reserve_drawn_v1(original, projection, &terms, schedule, u64::MAX, &mut unchanged, maximum_packets, price),
                Err(BatchCourtErrorV1::Reservation(ServiceRefusal::DeadlineExhausted))
            ));
            assert_eq!(unchanged, empty_book, "late binding/delivery refusal cannot leave a sibling rank reserved");
        }
    }

    check_companion_booking_parity(projection, &terms, fixture, at, notice, case, &empty_book, maximum_packets);

    let bytes = super::super::manifest_carrier::MANIFEST_REQUEST_CARRIER_BYTES_V1 as u64;
    let retained = InitialMaterialRequestPlanV1::minimum_retained_bytes_v1();
    let initial_cost = ServiceCost { bytes, work: bytes, io: retained, controls: 1 };
    let initial_plan = InitialMaterialRequestPlanV1::new_v1(at.daa_score, notice, initial_cost, retained).unwrap();
    check_protected_role_booking(projection, projected, rescue, initial_plan, schedule, original, width);
    let with_initial = terms.with_initial_request_v2(initial_plan).unwrap();
    assert_ne!(with_initial.obligation_v1(), terms.obligation_v1());
    assert_ne!(with_initial.relative_graph, terms.relative_graph);
    let complete_cap = maximum_packets + rescue.max_drawn_bonds as usize;
    let mut complete_book = empty_book.clone();
    let (complete_quotes, complete_graph) =
        reserve_drawn_v1(original, projection, &with_initial, schedule, u64::MAX, &mut complete_book, complete_cap, price).unwrap();
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
    assert!(reserve_drawn_v1(original, projection, &with_initial, schedule, u64::MAX, &mut refused, complete_cap - 1, price).is_err());
    assert_eq!(refused, empty_book);
    check_companion_booking_parity(projection, &with_initial, fixture, at, notice, case, &empty_book, complete_cap);

    // A wider request interval permits two different readiness floors with
    // the same deadline. Signed and unsigned paths must bind both ends.
    let wide_notice = FirstNoticeDemand { material_request_inclusion_daa: 2, ..notice };
    let wide_plan = InitialMaterialRequestPlanV1::new_v1(at.daa_score, wide_notice, initial_cost, retained).unwrap();
    let wide_terms = terms.with_initial_request_v2(wide_plan).unwrap();
    check_companion_booking_parity(projection, &wide_terms, fixture, at, wide_notice, case, &empty_book, complete_cap);
}

fn protected_test_book() -> ServiceReservations {
    // These fixture windows can align all four graphs. Give this test room
    // to exercise role accounting rather than deliberately saturating a slot.
    ServiceReservations::new(ServiceCost { bytes: 100_000_000, work: 100_000_000, io: 100_000_000, controls: 4096 }).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn check_protected_role_booking(
    projection: &PlainBatchOriginalProjectionV2,
    projected: PlainBatchProspectiveOriginalTermsV2,
    drawn_rescue: ProtectedRescueDemand,
    initial_request: InitialMaterialRequestPlanV1,
    schedule: VerificationSchedule,
    original: OriginalId,
    width: usize,
) {
    use ProtectedBatchRoleV1::*;
    let empty = &protected_test_book();
    let rescue = ProtectedRescueDemand { max_outsiders: 1, per_outsider: drawn_rescue.per_drawn_bond, ..drawn_rescue };
    let drawn = DrawnBatchPreclaimTermsV1::new_v1(projected, rescue).unwrap().with_initial_request_v2(initial_request).unwrap();
    let terms = ProtectedBatchPreclaimTermsV1::new_v1(projected, rescue, Some(initial_request)).unwrap();
    assert_ne!(terms.obligation_v1(), drawn.obligation_v1());
    assert_eq!(terms.relative_graph_id_v1(), drawn.relative_graph);
    let coordinates = [(Drawn, 0), (Drawn, 1), (Outsider, 0), (Defense, 0)];
    let scopes = coordinates.map(|(role, rank)| terms.reservation_scope_v1(original, role, rank).unwrap());
    assert_eq!(scopes.into_iter().collect::<BTreeSet<_>>().len(), coordinates.len());
    assert_ne!(scopes[0], drawn_scope_v1(drawn.obligation_v1(), original, 0, rescue.max_drawn_bonds).unwrap());
    for (role, invalid_rank) in [(Drawn, 2), (Outsider, 1), (Defense, 1)] {
        assert_eq!(terms.reservation_scope_v1(original, role, invalid_rank), Err(ContractError::Budget));
    }

    let maximum_packets = (width + 1) * coordinates.len();
    let price = |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
    let reserve = |terms: &ProtectedBatchPreclaimTermsV1, fee, cap, book: &mut ServiceReservations| {
        reserve_batch_v1(original, projection, &terms.batch, BookingScopeV1::Protected, schedule, fee, None, book, cap, price)
    };
    let mut booked = empty.clone();
    let (quotes, graph) = reserve(&terms, u64::MAX, maximum_packets, &mut booked).unwrap();
    assert_eq!(quotes.iter().map(|quote| quote.scope).collect::<Vec<_>>(), scopes);
    assert_eq!(graph.len(), maximum_packets);
    let ids: BTreeSet<_> = graph.iter().map(|node| node.packet.id).collect();
    assert_eq!(ids.len(), maximum_packets);
    assert!(ids.iter().all(|id| booked.assigned_daa(id).is_some()));
    for quote in quotes.iter() {
        let request = initial_request.request_v1(quote.claim, quote.scope, projected.accepted_daa_v2()).unwrap();
        assert!(ids.contains(&request.packet.id));
        assert!(graph.iter().any(|node| node.predecessors.contains(&request.packet.id)));
    }

    let fee_floor = batch_storage_fee_v1(&quotes).unwrap();
    assert!(fee_floor > 0);
    let mut exact = empty.clone();
    reserve(&terms, fee_floor, maximum_packets, &mut exact).unwrap();
    assert_eq!(exact, booked);
    let mut refused = empty.clone();
    assert!(matches!(
        reserve(&terms, fee_floor - 1, maximum_packets, &mut refused),
        Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
    ));
    assert_eq!(&refused, empty);
    assert!(matches!(reserve(&terms, fee_floor, maximum_packets - 1, &mut refused), Err(BatchCourtErrorV1::CaseGraph)));
    assert_eq!(&refused, empty);

    // These short allowances pass the Trace-only floor and fail against the
    // relevant role's graph in the common pre-commit callback.
    let short = RemainingCapacity { additional_work: 1, ..rescue.per_drawn_bond };
    for underfunded in [
        ProtectedRescueDemand { per_drawn_bond: short, ..rescue },
        ProtectedRescueDemand { per_outsider: short, ..rescue },
        ProtectedRescueDemand { defensive: short, ..rescue },
    ] {
        let changed = ProtectedBatchPreclaimTermsV1::new_v1(projected, underfunded, Some(initial_request)).unwrap();
        assert_ne!(changed.obligation_v1(), terms.obligation_v1());
        assert!(matches!(
            reserve(&changed, u64::MAX, maximum_packets, &mut refused),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(&refused, empty);
    }
}

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
    let scope = ChainScope { network: Digest(fp.job.network_domain.as_bytes()), ruleset: d, branch: d };
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
    // Derive the signed storage floor from the native graph, not a fee
    // fixture. All other profile duties still need additional funding.
    let price = |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 });
    let schedule = VerificationSchedule::new(OriginalClock::new(at.daa_score).unwrap(), notice, case).unwrap();
    let mut priced_book = empty_book.clone();
    let (priced, _) =
        reserve_drawn_v1(OriginalId(d), projection, terms, schedule, u64::MAX, &mut priced_book, maximum_packets, price).unwrap();
    let fee_floor = batch_storage_fee_v1(&priced).unwrap();
    assert!(fee_floor > 0);
    let budget = AggregateBudget {
        rounds: 1,
        unique_source_bytes: 100_000_000,
        transmitted_bytes: capacity.transmitted_bytes,
        additional_work: capacity.additional_work,
        retained_bytes: capacity.retained_bytes,
        retention_until: terms.projected.material_until_daa_v2(),
        fee: fee_floor,
        witness_attempts: 1,
    };
    let obligation = terms.obligation_v1();
    let service_contract = obligation.service_contract_v1(&budget);
    let source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_v1(
        &fixture.record,
        &fixture.layout,
        frame.header_v1(),
        fp.execution_root,
        fp.trace_root,
        fixture.rules.maximum_leaves,
    )
    .unwrap();
    let policy = super::super::Tfp1PublicAdmissionPolicyV1 {
        scope,
        max_manifest_bytes: ManifestV1::WIRE_BYTES,
        manifest_scope_limits: &limits,
        manifest_total_limit: 64 + 128 + 68 + MaterialScope::COUNT as u64 - 3,
    };
    let admission =
        PublicOriginalAdmissionV1::from_native_tfp1_v1(fp, &source, &binding.class, policy, b"fixture prompt", obligation, &budget)
            .unwrap();
    assert_eq!(admission.initial_state, initial_state);
    assert_eq!(admission.dependencies, dependencies);
    assert_eq!(admission.prompt, prompt);
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
    let mut unsigned_book = empty_book.clone();
    let (unsigned_quotes, unsigned_graph) = admission
        .reserve_tfp1_drawn_batch_candidate_v1(
            &companion,
            &reservation.prospective_v2(),
            projection,
            terms,
            &mut unsigned_book,
            maximum_packets,
            price,
        )
        .unwrap();
    let mut signed_book = empty_book.clone();
    let (signed_quotes, signed_graph) =
        prepared.reserve_drawn_batch_candidates_v1(projection, terms, &mut signed_book, maximum_packets, price).unwrap();
    assert_eq!(unsigned_quotes, signed_quotes);
    assert_eq!(unsigned_graph, signed_graph);
    assert_eq!(unsigned_book, signed_book);

    assert_eq!(batch_storage_fee_v1(&unsigned_quotes).unwrap(), fee_floor);

    let protected = ProtectedBatchPreclaimTermsV1::new_v1(terms.projected, terms.rescue, terms.initial_request).unwrap();
    let protected_empty = protected_test_book();
    let protected_packet_cap = maximum_packets + maximum_packets / terms.rescue.max_drawn_bonds as usize;
    let mut priced_book = protected_empty.clone();
    let (priced, _) = reserve_batch_v1(
        OriginalId(d),
        projection,
        &protected.batch,
        BookingScopeV1::Protected,
        schedule,
        u64::MAX,
        None,
        &mut priced_book,
        protected_packet_cap,
        price,
    )
    .unwrap();
    let protected_budget = AggregateBudget { fee: batch_storage_fee_v1(&priced).unwrap(), ..budget };
    let protected_obligation = protected.obligation_v1();
    let protected_admission = PublicOriginalAdmissionV1::from_native_tfp1_v1(
        fp,
        &source,
        &binding.class,
        policy,
        b"fixture prompt",
        protected_obligation,
        &protected_budget,
    )
    .unwrap();
    let protected_companion = OriginalCommit { service_contract: protected_admission.service_contract, ..companion.clone() };
    let protected_reservation = PublicOriginalReservationV1 {
        service_obligation: protected_obligation.obligation_v1(),
        budget: &protected_budget,
        ..reservation
    };
    let protected_prepared = protected_admission
        .prepare_tfp1_signed_bytes_v1(&sign_companion(&protected_companion), protected_obligation, &protected_reservation)
        .unwrap();
    // Exercise the new combined reservation with the same real companion
    // signing key. Early duty prices/geometry here are reservation fixtures,
    // not a complete profile or evidence of successful native delivery.
    if let Some(initial) = terms.initial_request {
        let duties = ProtectedOriginalDutiesV1::reservation_fixture_v1(terms, at.blue_score, schedule);
        let combined =
            ProtectedBatchPreclaimTermsV1::new_with_original_duties_v2(terms.projected, terms.rescue, initial, duties).unwrap();
        let last_notice = FirstNoticeDemand { missing_material_detected_by: at.daa_score + 3119, material_request_inclusion_daa: 1, ..notice };
        let first_packet = initial.request_v1(fp_claim_id_v3(fp), Hash64::from_u64_word(1), at.daa_score).unwrap();
        let last = InitialMaterialRequestPlanV1::new_v1(at.daa_score, last_notice, first_packet.packet.cost, first_packet.retained_bytes).unwrap();
        let single = ProtectedBatchPreclaimTermsV1::new_with_original_duties_v2(terms.projected, terms.rescue, last, duties).unwrap();
        let series = ProtectedBatchPreclaimTermsV1::new_with_request_series_v3(terms.projected, terms.rescue, MaterialRequestSeriesV2::new_v2(last, 256).unwrap(), duties).unwrap();
        assert_eq!(single.relative_graph_id_v1(), series.relative_graph_id_v1());
        assert_ne!(single.obligation_v1(), series.obligation_v1(), "series receipt semantics must be signed even with one slot");
        let obligation = combined.obligation_v1();
        assert_ne!(obligation, protected_obligation);
        let notice_packets = usize::try_from(terms.rescue.max_drawn_bonds + terms.rescue.max_outsiders).unwrap();
        assert_eq!(duties.count_v1().unwrap(), notice_packets, "off-chain material bytes do not create service packets");
        let cap = protected_packet_cap + notice_packets;
        let fee = protected_budget.fee.checked_add(duties.fees_v1().total_v1().unwrap()).unwrap();
        let resolve = |fee| {
            let budget = AggregateBudget { fee, ..protected_budget };
            let admission = PublicOriginalAdmissionV1::from_native_tfp1_v1(
                fp,
                &source,
                &binding.class,
                policy,
                b"fixture prompt",
                obligation,
                &budget,
            )
            .unwrap();
            let companion = OriginalCommit { service_contract: admission.service_contract, ..protected_companion.clone() };
            (budget, admission, companion)
        };
        let (combined_budget, combined_admission, combined_companion) = resolve(fee);
        let combined_reservation =
            PublicOriginalReservationV1 { service_obligation: obligation.obligation_v1(), budget: &combined_budget, ..reservation };
        let prepared = combined_admission
            .prepare_tfp1_signed_bytes_v1(&sign_companion(&combined_companion), obligation, &combined_reservation)
            .unwrap();
        let mut unsigned = protected_empty.clone();
        let mut signed = protected_empty.clone();
        let quote = combined_admission
            .reserve_tfp1_protected_batch_candidate_v1(
                &combined_companion,
                &combined_reservation.prospective_v2(),
                projection,
                &combined,
                &mut unsigned,
                cap,
                price,
            )
            .unwrap();
        let booking = prepared.reserve_protected_batch_candidates_v1(projection, &combined, &mut signed, cap, price).unwrap();
        assert_eq!(quote.graph_v1(), booking.graph_v1());
        assert_eq!(quote.quotes_v1(), booking.quotes_v1());
        assert_eq!(quote.early_fees_v1(), Some(duties.fees_v1()));
        assert_eq!(booking.early_fees_v1(), quote.early_fees_v1());
        assert_eq!(booking.graph_v1().len(), cap);
        assert_eq!(unsigned, signed);
        assert!(booking.check_book_v1(&signed).is_ok());
        for node in duties.graph_v1(combined_companion.id()).unwrap().iter() {
            assert_eq!(signed.packet(&node.packet.id), Some(&node.packet));
            assert!(signed.assigned_daa(&node.packet.id).unwrap() <= schedule.first_notice_by());
        }
        let mut refused = protected_empty.clone();
        let short_http = PublicOriginalProspectiveReservationV2 {
            initial_supply: RemainingCapacity { transmitted_bytes: (1 << 20) - 1, ..initial_supply },
            ..combined_reservation.prospective_v2()
        };
        assert!(matches!(
            combined_admission.reserve_tfp1_protected_batch_candidate_v1(
                &combined_companion,
                &short_http,
                projection,
                &combined,
                &mut refused,
                cap,
                price,
            ),
            Err(BatchCourtErrorV1::CaseGraph)
        ));
        assert_eq!(refused, protected_empty, "off-chain underbudget fails before booking");
        assert!(matches!(
            prepared.reserve_protected_batch_candidates_v1(projection, &combined, &mut refused, cap - 1, price),
            Err(BatchCourtErrorV1::CaseGraph)
        ));
        assert_eq!(refused, protected_empty);
        let (short_budget, short_admission, short_companion) = resolve(fee - 1);
        let short_reservation = PublicOriginalReservationV1 { budget: &short_budget, ..combined_reservation };
        let short =
            short_admission.prepare_tfp1_signed_bytes_v1(&sign_companion(&short_companion), obligation, &short_reservation).unwrap();
        assert!(matches!(
            short.reserve_protected_batch_candidates_v1(projection, &combined, &mut refused, cap, price),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(refused, protected_empty);
        assert!(matches!(
            short_admission.reserve_tfp1_protected_batch_candidate_v1(
                &short_companion,
                &short_reservation.prospective_v2(),
                projection,
                &combined,
                &mut refused,
                cap,
                price,
            ),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(refused, protected_empty);
    }

    let mut protected_unsigned = protected_empty.clone();
    let mut protected_signed = protected_empty.clone();
    let unsigned_booking = protected_admission
        .reserve_tfp1_protected_batch_candidate_v1(
            &protected_companion,
            &protected_reservation.prospective_v2(),
            projection,
            &protected,
            &mut protected_unsigned,
            protected_packet_cap,
            price,
        )
        .unwrap();
    let signed_booking = protected_prepared
        .reserve_protected_batch_candidates_v1(projection, &protected, &mut protected_signed, protected_packet_cap, price)
        .unwrap();
    assert_eq!(unsigned_booking.quotes_v1(), signed_booking.quotes_v1());
    assert_eq!(unsigned_booking.graph_v1(), signed_booking.graph_v1());
    for booking in [&unsigned_booking, &signed_booking] {
        assert_eq!(booking.check_book_v1(&protected_signed), Ok(()));
        assert!(booking.check_book_v1(&protected_empty).is_err());
        let mut substituted = protected_signed.clone();
        substituted.budget.work += 1;
        assert!(booking.check_book_v1(&substituted).is_err());
        let mut substituted = protected_signed.clone();
        *substituted.assignments.values_mut().next().unwrap() += 1;
        assert!(booking.check_book_v1(&substituted).is_err());
        assert_eq!(booking.original_v1(), protected_companion.id());
        assert_eq!(booking.obligation_v1(), protected_obligation);
        assert!(booking.matches_acceptance_v1(at.daa_score, at.blue_score));
        assert!(!booking.matches_acceptance_v1(at.daa_score + 1, at.blue_score));
        assert!(!booking.matches_acceptance_v1(at.daa_score, at.blue_score + 1));
    }
    assert_eq!(protected_unsigned, protected_signed);
    assert_ne!(protected_unsigned, protected_empty);
    assert_eq!(unsigned_booking.quotes_v1().len(), terms.rescue.max_drawn_bonds as usize + 1);
    assert!(protected.reservation_scope_v1(protected_companion.id(), ProtectedBatchRoleV1::Outsider, 0).is_err());

    // Quote the same native graph under a budget that includes the separately
    // resolved original fee. Payer consent is checked on the actual TFO3 output;
    // future provider-carrier rent is not charged into that output twice.
    let funding_budget = AggregateBudget { fee: protected_budget.fee + 7, ..protected_budget };
    let funding_admission = PublicOriginalAdmissionV1::from_native_tfp1_v1(
        fp,
        &source,
        &binding.class,
        policy,
        b"fixture prompt",
        protected_obligation,
        &funding_budget,
    )
    .unwrap();
    let funding_companion = OriginalCommit { service_contract: funding_admission.service_contract, ..protected_companion };
    let funding_reservation = PublicOriginalReservationV1 { budget: &funding_budget, ..protected_reservation };
    let mut funding_book = protected_empty.clone();
    let funding_booking = funding_admission
        .reserve_tfp1_protected_batch_candidate_v1(
            &funding_companion,
            &funding_reservation.prospective_v2(),
            projection,
            &protected,
            &mut funding_book,
            protected_packet_cap,
            price,
        )
        .unwrap();
    super::super::funding::tests::exercise_funding_v1(
        fixture,
        &source,
        &funding_admission,
        &funding_reservation,
        &funding_companion,
        &protected,
        &funding_booking,
        super::super::funding::Tfp1FundingPolicyV1 {
            profiles: [Hash64::from_u64_word(940), Hash64::from_u64_word(941), Hash64::from_u64_word(942), Hash64::from_u64_word(943)],
            other_relative_graph: Hash64::from_u64_word(944),
            original_fee_sompi: 7,
            capital_floor_sompi: 11,
        },
        projection,
        &protected_empty,
        protected_packet_cap,
        &sign_companion,
    );

    // An exact budget succeeds above; one missing unit or one already spent
    // unit must reject both paths without leaving any rank booked.
    for already_used in [false, true] {
        let tight_budget = AggregateBudget { fee: fee_floor - u64::from(!already_used), ..budget };
        let tight_usage = AggregateUsage { fee: u64::from(already_used), ..usage };
        let tight_admission = PublicOriginalAdmissionV1::from_native_tfp1_v1(
            fp,
            &source,
            &binding.class,
            policy,
            b"fixture prompt",
            obligation,
            &tight_budget,
        )
        .unwrap();
        let tight_companion = OriginalCommit { service_contract: tight_admission.service_contract, ..companion.clone() };
        let tight_reservation = PublicOriginalReservationV1 { usage: &tight_usage, budget: &tight_budget, ..reservation };
        let tight_signed = sign_companion(&tight_companion);
        let tight_prepared = tight_admission.prepare_tfp1_signed_bytes_v1(&tight_signed, obligation, &tight_reservation).unwrap();
        let mut refused = empty_book.clone();
        assert!(matches!(
            tight_admission.reserve_tfp1_drawn_batch_candidate_v1(
                &tight_companion,
                &tight_reservation.prospective_v2(),
                projection,
                terms,
                &mut refused,
                maximum_packets,
                price,
            ),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(&refused, empty_book);
        assert!(matches!(
            tight_prepared.reserve_drawn_batch_candidates_v1(projection, terms, &mut refused, maximum_packets, price,),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(&refused, empty_book);
    }

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
                &later_reservation.prospective_v2(),
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
            &wrong_blue.prospective_v2(),
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
            &short_reservation.prospective_v2(),
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
