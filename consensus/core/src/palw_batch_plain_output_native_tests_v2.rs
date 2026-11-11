#[test]
fn bct2_paid_plain_output_verify_receipt_and_fault_terminal() {
    exercise_bct2_paid_plain_output_v2("bct2_paid_plain_output_verify_receipt_and_fault_terminal", true);
}

#[test]
fn bct2_paid_plain_output_holds_keeps_original_and_refuses_terminal() {
    exercise_bct2_paid_plain_output_v2("bct2_paid_plain_output_holds_keeps_original_and_refuses_terminal", false);
}

fn exercise_bct2_paid_plain_output_v2(test_name: &str, faults: bool) {
    use crate::config::params::ForkActivation;
    use crate::palw_native_resource_v1::{native_logical_admission_enabled_v1, native_resource_admission_enabled_v1};
    const CHILD: &str = "MISAKA_TEST_BCT2_DEFAULT_PROCESS_OWNER";
    if std::env::var_os(CHILD).is_none() {
        let name = format!("{}::{test_name}", module_path!());
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    assert!(!native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1());
    use crate::palw_batch_court_v1::{
        carriage::{
            BATCH_COURT_CARRIER_MAGIC_V1, BATCH_DEMAND_SIGNATURE_CONTEXT_V1, BatchCourtWireV1, BatchMaterialControlV1,
            BatchPlainOutputTerminalV2, BatchPlainOutputWireV2, PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2,
            encode_batch_plain_output_payload_v2, quote_batch_plain_output_full_ids_open_v2, quote_batch_plain_output_terminal_v2,
            quote_batch_plain_output_verify_v2,
        },
        plain_output_transport::BatchPlainOutputTransportV2,
    };
    use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
    use crate::palw_court_policy_v1::NOTICE_WINDOW_DAA;
    use crate::palw_court_service_release_v1::CourtServiceRowV3;
    use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding, court_service_profile_id_v2};
    use crate::palw_court_service_v1::{ServiceCost, ServiceReservations};
    use crate::palw_court_shared_material_intake_v1::{
        MATERIAL_BATCH_CONTEXT_V1, MATERIAL_BATCH_VERSION, MaterialActionV1, MaterialTargetV1, SignedMaterialIntakeV1,
        ValidatedBatchCarrierV1,
        batch::{
            admission::{
                CompiledBatchAdmissionV1, NativeBatchDeliveryCostsV1, NativeBatchDeliveryShapeV1, NativeBatchDeliveryTimingV1,
                batch_numerical_service_ids_v1, compile_native_batch_plain_output_v2,
            },
            numerical_verification::NumericalBatchReplayBudgetV1,
            plain_output::{BATCH_PLAIN_OUTPUT_ORDINAL_V2, PlainBatchOutputV2, batch_plain_output_control_id_v2},
        },
        material_intake_schedule_v1, quote_batch_response_v1,
    };
    use crate::palw_court_shared_tir_frontier_v1::{SHARED_REQUEST_CONTEXT_V1, SignedSharedRequestV1};
    use crate::palw_court_shared_tir_state_v1::{derive_plain_batch_tir_context_v2, plain_batch_profile_id_v2};
    use crate::palw_court_tir_requests_v1::TirSharedRequestSetV1;
    use crate::palw_history_custody_v1::{PALW_HISTORY_PLEDGE_CONTEXT_V1, PalwHistoryCustodyPledgeV1};
    use crate::palw_tir_container_v1::{check_tir_container_binding_v1, test_binding_v1};
    use crate::palw_tir_public_claim_v1::{
        palw_tir_public_tokenizer_frame_fixture_v1, palw_tir_public_tokenizer_frame_lying_output_fixture_v2,
    };
    use crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1;
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicResponseV1};
    use crate::palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, palw_original_tokenizer_registration_for_fold_v2};
    use crate::tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction,
    };

    // The fixture fixes the output commitment before building the original.
    // Class and custody use native fixture setup. The tokenizer and every
    // Batch control below go through their normal writers.
    let (fx, semantics, eof, raw, table) = if faults {
        palw_tir_public_tokenizer_frame_lying_output_fixture_v2(h(0xBC72))
    } else {
        palw_tir_public_tokenizer_frame_fixture_v1()
    };
    let frame = &fx.frame;
    let plan = frame.plan_v1();
    let a = plan.terms.accepted_daa;
    let claim = plan.original_claim;
    let class = semantics.class_id;
    let (mut state, params, mut extras) = world(&fx, frame);
    let params = params.with_tir_from_daa(Some(0)).with_court_clock_v2_from_daa(Some(a));
    let binding = test_binding_v1(&fx.record, semantics.artifact_root, b"bct2-funded-output");
    let container = check_tir_container_binding_v1(&binding, &class, &semantics.artifact_root, &fx.record, 1).unwrap();
    let pins = PalwTfp1TokenizerIntakePinsV2 {
        class_id: class,
        container_root: container.container_root,
        semantics_id: semantics.commitment(),
        inventory_id: eof.inventory_id,
    };
    state.class_semantics.insert(class, semantics.clone());
    state.claim_semantics.insert(claim, semantics.clone());
    state.staged_courts.insert(Key::Inventory(eof.artifact_hash), Row::ArtifactInventory(eof));
    state.staged_courts.insert(Key::TirContainer(class), Row::TirContainer(Box::new(container)));
    let filer = outsider();
    let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([177; 32]);
    state.bonds.get_mut(&filer).unwrap().pubkey = keys.verification_key.as_ref().to_vec();
    let sign = |message: Hash64, purpose: &[u8]| {
        libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, message.as_byte_slice(), purpose, [178; 32]).unwrap()
    };
    extras.class_semantics_active = true;
    extras.tir_container_identity_active = true;
    extras.tir_public_conviction_active = true;
    extras.staged_public_court = Some(fx.rules.config);
    extras.staged_tir_route_active = true;
    extras.batch_court_active_v1 = true;
    let network = extras.tir_public_network_domain_v1.unwrap();
    let transport = BatchPlainOutputTransportV2 {
        activation: ForkActivation::new(a),
        maximum_source_blocks: 1,
        maximum_source_controls: 4,
        maximum_work_per_pass: 1 << 28,
        maximum_io_per_pass: 1 << 28,
        maximum_workspace_per_carrier: 64 << 20,
        maximum_prefix_bytes: 256 << 20,
        maximum_prefix_work_per_byte: 1,
    };
    extras.batch_plain_output_transport_v2 = Some(transport);
    let mut registration_parent = state.clone();
    registration_parent.claims.remove(&claim);
    registration_parent.claim_semantics.remove(&claim);
    registration_parent.staged_courts.remove(&frame.key_v1());
    registration_parent.last_point =
        Some(PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(8810), daa_score: 10, blue_score: 10, subsidy: 0 });
    let registration_point =
        PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(8811), daa_score: 11, blue_score: 11, subsidy: 0 };
    let mut registration = TransitionBuilder::new(&registration_parent, &params, false, false, false, false, &extras);
    registration
        .install_verified_tokenizer_registration_v1(
            &registration_parent,
            registration_point,
            pins,
            &raw,
            &table.eog_opening().unwrap(),
        )
        .unwrap();
    let key = Key::VerifiedTokenizerRegistration(class, pins.semantics_id);
    state.staged_courts.insert(key, registration.state.staged_court_row_v1(&key).unwrap().clone());
    state.last_point = Some(at(frame, 0));
    let producer = frame.custody_v1().pledge_v1().producer;
    let producer_keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([179; 32]);
    state.bonds.get_mut(&producer).unwrap().pubkey = producer_keys.verification_key.as_ref().to_vec();
    let mut pledge = PalwHistoryCustodyPledgeV1 {
        version: 1,
        claim,
        producer,
        execution_root: frame.original_v1().execution_root,
        job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(frame.original_v1()),
        cu_ruleset_id: semantics.cu_ruleset_id,
        resource_plan_id: plan.semantic_resource_plan_id.unwrap(),
        maximum_records: plan.maximum_semantic_history_records,
        maximum_custodians: plan.acquisition.maximum_custodians,
        accept_until_daa: a + 1,
        material_until_daa: plan.terms.material_until_daa,
        signature: vec![],
    };
    pledge.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &producer_keys.signing_key,
        pledge.pledge_id_v1().as_byte_slice(),
        PALW_HISTORY_PLEDGE_CONTEXT_V1,
        [180; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    extras.history_custody_pledges.insert(claim, pledge);
    let mut dependency = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let bundle = frame_semantic_bundle_v1(&dependency, frame, a).unwrap().unwrap();
    dependency.write_staged_row_v1(Key::Claim(claim), Some(bundle)).unwrap();
    let state = dependency.state;

    let source_profile_id = h(0xBC73);
    let mut service = CourtServiceRowV3::from_v2(&CourtServiceStateV2 {
        version: 2,
        source_profile_id,
        profile_id: court_service_profile_id_v2(source_profile_id),
        book: ServiceReservations::new(ServiceCost { bytes: 64 << 20, work: 1 << 20, io: 1 << 20, controls: 64 }).unwrap(),
        claims: std::collections::BTreeMap::from([(
            claim,
            ServiceClaimBinding {
                claim,
                allocation_id: plan.allocation_id,
                source_id: plan.source_id,
                original_accepted_daa: a,
                hard_deadline_daa: a + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA,
                packet_ids: Default::default(),
            },
        )]),
    });
    let registration_id = palw_original_tokenizer_registration_for_fold_v2(&state, claim).unwrap().unwrap().record_id_v1();
    let profile = plain_batch_profile_id_v2(service.state.profile_id, registration_id).unwrap();
    let source = frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
    let set = TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
    let scope = set.scope_id().unwrap();
    let cost = |bytes| ServiceCost { bytes, work: 1, io: 1, controls: 1 };
    let output = PlainBatchOutputV2::derive_v2(frame.original_v1(), &source, cost(8192), 2).unwrap();
    let replay = NumericalBatchReplayBudgetV1::test_only_v1();
    let costs = NativeBatchDeliveryCostsV1 {
        numerical_verification: replay,
        terminal: ServiceCost { bytes: 8192, ..replay.service_cost_v1().unwrap() },
        recovery_daa: 1,
        maximum_packets: 128,
        timing: NativeBatchDeliveryTimingV1 {
            open_daa: 2,
            provider_wait_daa: 2,
            begin_daa: 2,
            parts_daa: 2,
            finish_daa: 3,
            verification_daa: 2,
            stop_daa: 1,
            terminal_daa: 2,
        },
    };
    let storage = material_intake_schedule_v1().unwrap();
    let compiled = compile_native_batch_plain_output_v2(
        NativeBatchDeliveryShapeV1 {
            claim,
            scope,
            accepted_daa: a,
            material_until_daa: plan.terms.material_until_daa,
            maximums: &[8192],
        },
        &output,
        costs,
        storage,
        |_, bytes| Ok(cost(bytes)),
    )
    .unwrap();
    let (_, graph) = CompiledBatchAdmissionV1::reserve_v1(compiled, &mut service.state.book).unwrap();
    let binding = service.state.claims.get_mut(&claim).unwrap();
    for node in graph.iter() {
        binding.packet_ids.insert(node.packet.id);
        if !node.predecessors.is_empty() {
            let mut predecessors = node.predecessors.clone();
            predecessors.sort_unstable();
            service.causal.insert(node.packet.id, predecessors);
        }
    }
    service.check().unwrap();
    let control = batch_plain_output_control_id_v2(claim, scope).unwrap();
    let verify_at = at(frame, service.state.book.assigned_daa(&control).unwrap() - a);
    let mut install = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    install.write_staged_row_v1(Key::CourtService, Some(Row::CourtServiceV3(Box::new(service).into()))).unwrap();
    let context = derive_plain_batch_tir_context_v2(&install.state, claim).unwrap();
    assert_eq!((context.version, context.scope_id), (3, scope));
    install.write_staged_row_v1(Key::CourtSharedContext(claim), Some(Row::CourtSharedContext(Box::new(context)))).unwrap();
    let state = install.state;

    let rules_at = |at| NativeCourtRulesV1 {
        network_domain: network,
        at,
        court: fx.rules.court,
        step_ladder: fx.rules.maximum_leaves,
        prompt_ids_merkle: false,
        held_regime: false,
        decode_close_convicts_only: false,
        fp_decode_from_daa: None,
        cell_closure_from_daa: None,
        memory_routing_from_daa: None,
        tir_active: true,
        gen_active: false,
        task_heads_active: false,
        improvement_active: false,
        constraint_active: false,
    };
    extras.native_court_rules_v1 = Some(rules_at(verify_at));
    let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);

    let ordinal =
        (0..set.len()).find(|&ordinal| set.request_at(ordinal).unwrap() == PalwTirPublicRequestV1::Ids { offset: 0 }).unwrap();
    let notice_id = set.request_id(ordinal).unwrap();
    let mut notice = SignedSharedRequestV1 { version: 1, scope, ordinal, request_id: notice_id, signer: filer, signature: vec![] };
    notice.signature = sign(notice.signing_message(network).unwrap(), SHARED_REQUEST_CONTEXT_V1).as_ref().to_vec();
    let notice_at = at(frame, NOTICE_WINDOW_DAA);
    super::super::palw_court_shared_tir_fold_v1::request(&mut builder, &notice_at, claim, &notice).unwrap();
    builder.state.last_point = Some(notice_at);

    let funded_carrier = |point: PalwBlockContextV2, payload, fee| {
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(point.block, 0), vec![], 0, 0)],
            vec![TransactionOutput::new(1, ScriptPublicKey::default())],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        let entry = UtxoEntry::new(1 + fee, ScriptPublicKey::default(), 1, false);
        let validated = ValidatedTransaction::new(PopulatedTransaction::new(&tx, vec![entry]), fee);
        ValidatedBatchCarrierV1::prepare_at_ordinal_versioned_v2(&validated, 0, point, 0, Some(transport)).unwrap().unwrap()
    };
    let apply = |builder: &mut TransitionBuilder<'_>, point: PalwBlockContextV2, payload, fee| {
        let carrier = funded_carrier(point, payload, fee);
        builder.apply_batch_court_carrier_v1(&point, &carrier).unwrap();
        builder.state.last_point = Some(point);
        carrier.fee_v1()
    };
    let encode_v1 = |wire| {
        let mut payload = BATCH_COURT_CARRIER_MAGIC_V1.to_vec();
        payload.extend(borsh::to_vec(&wire).unwrap());
        payload
    };
    let demand_ordinal = BATCH_PLAIN_OUTPUT_ORDINAL_V2;
    let (count, mut open) = quote_batch_plain_output_full_ids_open_v2(&builder.state, claim, notice_id).unwrap();
    assert_eq!(count, source.decode_count_v1());
    assert_eq!(open.ordinal, demand_ordinal);
    assert!(quote_batch_plain_output_full_ids_open_v2(&builder.state, claim, h(0xBC75)).is_err());
    let open_at = at(frame, open.demand.opened_daa - a);
    use crate::palw_user_views_v1::palw_batch_plain_output_full_ids_open_json_v2 as full_ids_view;
    let open_view = full_ids_view(&builder.state, claim, notice_id, notice_at.daa_score, |_| Some(rules_at(open_at)));
    assert_eq!(open_view["fullIdsCount"].as_u64(), Some(u64::from(count)));
    assert_eq!(open_view["networkDomain"].as_str(), Some(network.to_string().as_str()));
    assert_eq!(open_view["acceptanceConfirmed"].as_bool(), Some(false));
    assert!(full_ids_view(&builder.state, claim, notice_id, open_at.daa_score + 1, |_| Some(rules_at(open_at)))["open"].is_null());
    assert!(full_ids_view(&builder.state, claim, notice_id, open_at.daa_score, |_| None)["open"].is_null());
    open.signature = sign(open.signing_message_v1(network).unwrap(), BATCH_DEMAND_SIGNATURE_CONTEXT_V1).as_ref().to_vec();
    apply(&mut builder, open_at, encode_v1(BatchCourtWireV1::Open(open)), 7);
    let target = MaterialTargetV1::FullIds;
    let slots = quote_batch_response_v1(&builder.state, claim, demand_ordinal, target).unwrap();
    let [part] = slots.parts.as_slice() else { panic!("the tiny fixture uses one FullIds part") };
    let respond = |action| {
        let mut intake = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim,
            scope_id: scope,
            target,
            submitter: filer,
            action,
            signature: vec![],
        };
        intake.signature =
            sign(intake.batch_signing_message_v1(network, slots.request_root).unwrap(), MATERIAL_BATCH_CONTEXT_V1).as_ref().to_vec();
        encode_v1(BatchCourtWireV1::Respond(BatchMaterialControlV1 { demand_ordinal, intake }))
    };
    let rows_root = crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
    let body = borsh::to_vec(&PalwTirPublicResponseV1::Ids { rows_root, ids: fx.execution.generated.clone() }).unwrap();
    assert_eq!(body.len() as u64, slots.total_bytes);
    let begin = MaterialActionV1::Begin { total_bytes: slots.total_bytes, body_hash: palw_tir_public_body_hash_v1(&body) };
    let body_rent = slots.total_bytes * storage.sompi_per_byte;
    let split = apply(&mut builder, at(frame, slots.begin_daa - a), respond(begin), body_rent + 7);
    assert_eq!((split.withheld_storage_rent, split.collectible_fee), (body_rent, 7));
    let submit = MaterialActionV1::Part { draft_id: slots.draft_id, offset: 0, bytes: body };
    apply(&mut builder, at(frame, part.assigned_daa - a), respond(submit), 7);
    apply(&mut builder, at(frame, slots.finish_daa - a), respond(MaterialActionV1::Finish { draft_id: slots.draft_id }), 7);
    assert!(quote_batch_response_v1(&builder.state, claim, demand_ordinal, target).unwrap().material_available);
    assert!(quote_batch_plain_output_terminal_v2(&builder.state, claim).is_err());

    let quote = quote_batch_plain_output_verify_v2(&builder.state, &params, &rules_at(verify_at), claim, filer).unwrap();
    let mut signed = quote.unsigned;
    signed.signature.copy_from_slice(sign(quote.signing_digest, PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2).as_ref());
    let verify = encode_batch_plain_output_payload_v2(&BatchPlainOutputWireV2::Verify(signed)).unwrap();
    let verify_rent = crate::palw_court_shared_material_intake_v1::batch::plain_output::BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES
        * storage.sompi_per_byte;
    let split = apply(&mut builder, verify_at, verify, verify_rent + 7);
    assert_eq!((split.withheld_storage_rent, split.collectible_fee), (verify_rent, 7));
    let Some(Row::CourtBatchPlainOutputReceiptV2(row)) =
        builder.state.staged_court_row_v1(&Key::CourtBatchPlainOutputReceiptV2(claim))
    else {
        panic!("successful Verify must root the receipt")
    };
    use crate::palw_tir_objective_v1::PalwTirOutputStopVerdictV1;
    use crate::palw_user_views_v1::palw_batch_plain_output_terminal_quote_json_v2 as terminal_view;
    let receipt_digest = row.receipt.digest_v2().unwrap();
    if !faults {
        assert_eq!(row.receipt.verdict, PalwTirOutputStopVerdictV1::Holds);
        assert!(quote_batch_plain_output_terminal_v2(&builder.state, claim).is_err());
        let (_, terminal) = batch_numerical_service_ids_v1(claim, scope).unwrap();
        let slot = builder.state.court_service_state_v1().unwrap().book.assigned_daa(&terminal).unwrap();
        let terminal_at = at(frame, slot - a);
        assert!(terminal_view(&builder.state, claim, slot, |_| Some(rules_at(terminal_at)))["terminal"].is_null());

        let control = BatchPlainOutputTerminalV2 { claim, receipt_digest };
        let payload = encode_batch_plain_output_payload_v2(&BatchPlainOutputWireV2::Terminal(control)).unwrap();
        let carrier = funded_carrier(terminal_at, payload, 7);
        let next = PalwTransitionExtrasV1 { native_court_rules_v1: Some(rules_at(terminal_at)), ..extras.clone() };
        let mut candidate = TransitionBuilder::new(&builder.state, &params, false, false, false, false, &next);
        assert!(matches!(
            candidate.apply_batch_court_carrier_v1(&terminal_at, &carrier),
            Err(PalwStateV2Error::CarriageInconsistent(why)) if why.contains("contains no fault")
        ));
        assert_eq!(candidate.state.state_root(), builder.state.state_root());
        assert!(candidate.entries.is_empty());
        assert!(matches!(candidate.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        return;
    }

    assert_eq!(row.receipt.verdict, PalwTirOutputStopVerdictV1::OutputRootFault);
    let terminal = quote_batch_plain_output_terminal_v2(&builder.state, claim).unwrap();
    assert_eq!(terminal.control.receipt_digest, receipt_digest);
    let terminal_at = at(frame, terminal.slot_daa - a);
    let quote = terminal_view(&builder.state, claim, verify_at.daa_score, |_| Some(rules_at(terminal_at)));
    assert_eq!(quote["terminal"]["assignedDaa"].as_u64(), Some(terminal.slot_daa));
    assert_eq!(quote["acceptanceConfirmed"].as_bool(), Some(false));
    assert!(terminal_view(&builder.state, claim, terminal.slot_daa + 1, |_| Some(rules_at(terminal_at)))["terminal"].is_null());
    assert!(terminal_view(&builder.state, claim, terminal.slot_daa, |_| None)["terminal"].is_null());

    let next = PalwTransitionExtrasV1 { native_court_rules_v1: Some(rules_at(terminal_at)), ..extras.clone() };
    let mut verdict = TransitionBuilder::new(&builder.state, &params, false, false, false, false, &next);
    let mut substituted = terminal.control;
    substituted.receipt_digest = h(0xBC74);
    let payload = encode_batch_plain_output_payload_v2(&BatchPlainOutputWireV2::Terminal(substituted)).unwrap();
    let refused = funded_carrier(terminal_at, payload, 7);
    let before = verdict.state.state_root();
    assert!(verdict.apply_batch_court_carrier_v1(&terminal_at, &refused).is_err());
    assert_eq!(verdict.state.state_root(), before);
    assert!(verdict.entries.is_empty());

    let lease = transport.reserve_replay_workspace_v2(&verdict.state, claim, true).unwrap().unwrap();
    assert!((1..=transport.maximum_workspace_per_carrier).contains(&lease.memory_v1()));
    drop(lease);
    let payload = encode_batch_plain_output_payload_v2(&BatchPlainOutputWireV2::Terminal(terminal.control)).unwrap();
    let carrier = funded_carrier(terminal_at, payload, 7);
    for limited in [
        BatchPlainOutputTransportV2 { maximum_prefix_bytes: 1, ..transport },
        BatchPlainOutputTransportV2 { maximum_workspace_per_carrier: 1, ..transport },
    ] {
        let local = PalwTransitionExtrasV1 { batch_plain_output_transport_v2: Some(limited), ..next.clone() };
        let mut small = TransitionBuilder::new(&builder.state, &params, false, false, false, false, &local);
        assert!(matches!(small.apply_batch_court_carrier_v1(&terminal_at, &carrier), Err(PalwStateV2Error::NativeResource(_))));
        assert_eq!(small.state.state_root(), before);
        assert!(small.entries.is_empty());
    }
    let original = TransitionBuilder::new(&builder.state, &params, false, false, false, false, &next);
    let retained = original.native_reservation.finish_state_envelope_v1(original.state);
    let (rehearsed, refused) = crate::palw_state_v2::palw_v2_apply_one_plain_batch_output_owned_v2(
        retained,
        &params,
        &terminal_at,
        &carrier,
        false,
        false,
        false,
        false,
        &next,
    )
    .unwrap();
    assert_eq!(refused, None);
    verdict.apply_batch_court_carrier_v1(&terminal_at, &carrier).unwrap();
    assert_eq!(rehearsed.state_root(), verdict.state.state_root());
    let split = carrier.fee_v1();
    assert_eq!((split.withheld_storage_rent, split.collectible_fee), (0, 7));
    assert!(matches!(
        verdict.state.claim(&claim).unwrap().phase,
        PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }
    ));
    let offence = palw_court_conviction_offence_id_v1(&producer.0, &claim);
    assert!(verdict.state.consumed_offences.contains_key(&offence));
}

#[test]
fn bct2_terminal_owns_nested_court_archive_rollback() {
    use crate::config::params::ForkActivation;
    use crate::palw_batch_court_v1::plain_output_transport::BatchPlainOutputTransportV2;
    use crate::palw_native_resource_v1::{
        NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, native_typed_value_requests_v1,
    };
    const CHILD: &str = "MISAKA_TEST_BCT2_ARCHIVE_OWNER";
    if std::env::var_os(CHILD).is_none() {
        let name = format!("{}::bct2_terminal_owns_nested_court_archive_rollback", module_path!());
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 64 << 20, database_bytes: 0 }));
    let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
    let frame = &fx.frame;
    let (mut state, params, mut extras) = world(&fx, frame);
    let params = params.with_tir_from_daa(Some(0));
    // This unrelated prefix makes a nested full-state clone distinguishable
    // from reservations made for the one archived session and its groups.
    for n in 0..512 {
        let key = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(20_000 + n), 0));
        state.bonds.insert(key, PalwBondStateV2 { pubkey: vec![7; 2_592], ..bond_state(n) });
    }
    let policy = BatchPlainOutputTransportV2 {
        activation: ForkActivation::new(0),
        maximum_source_blocks: 1,
        maximum_source_controls: 1,
        maximum_work_per_pass: 1 << 20,
        maximum_io_per_pass: 1 << 20,
        maximum_workspace_per_carrier: 2 << 20,
        maximum_prefix_bytes: 16 << 20,
        maximum_prefix_work_per_byte: 1,
    };
    extras.batch_plain_output_transport_v2 = Some(policy);
    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    archive_service(&mut b, frame);
    let (id, session) = archive_session(&mut b, frame);
    let state = b.state.clone();
    let prefix_bytes = native_typed_value_requests_v1(&state).unwrap();
    let journal = b.entries.len();

    let injected = "fixture refusal after court archive".to_string();
    let result = b.with_bounded_plain_output_checkpoint_v2(policy.maximum_prefix_bytes, |inner| {
        let requested = inner.native_reservation.requests_v1();
        inner.write_court(id, None)?;
        assert!(inner.state.court_session(&id).is_none());
        assert_eq!(inner.state.court_end_archive_v1(id).unwrap().session, session);
        assert!(inner.native_reservation.requests_v1() - requested < prefix_bytes);
        Err::<(), _>(PalwStateV2Error::CarriageInconsistent(injected.clone()))
    });
    assert!(matches!(result, Err(PalwStateV2Error::CarriageInconsistent(why)) if why == injected));
    assert_eq!(b.state, state);
    assert_eq!(b.entries.len(), journal);
    assert!(!b.plain_output_terminal_checkpoint_active_v2);

    b.with_bounded_plain_output_checkpoint_v2(policy.maximum_prefix_bytes, |inner| inner.write_court(id, None)).unwrap();
    assert!(b.state.court_session(&id).is_none());
    assert_eq!(b.state.court_end_archive_v1(id).unwrap().session, session);
    assert!(!b.plain_output_terminal_checkpoint_active_v2);
}
