        // G14 lane escrow-rfc08 round 2: the funded-original coordinator on the SAME real native original (written, not run).
        // Included inside `g14_native_funded_candidate_fixture_generator_v1` after the requirements block: `state`, `params`,
        // `extras`, `envelope` (the literal TFP1), `object`, `ctx`, `claim` and `fp` are that fixture's.
        {
            use crate::palw_court_original_escrow_v1::{FundingConsentV1, FundingTermsV1, COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1};
            use crate::palw_funded_original_coordinator_v1::*;
            use crate::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
            // Armed X3 mirror (test fixture policy, not a preset change) so the hard deadline is sourced.
            let x3 = crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1 {
                from_daa: 0,
                policy: crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(
                    crate::config::params::ForkActivation::new(0),
                )
                .policy,
            };
            let params = params.clone().with_claim_hard_deadline_v1(Some(x3));
            let hard = crate::palw_funded_escrow_v1::palw_funded_escrow_hard_deadline_v1(Some(x3), ctx.daa_score).unwrap();
            // Lane r5: the (test) court service profile the all-branch reservation runs under; the book is opened under it.
            let service_profile = crate::palw_court_service_binding_v1::CourtServiceProfileV1 {
                version: 1,
                budget: crate::palw_court_service_v1::ServiceCost { bytes: 1 << 40, work: 1 << 40, io: 1 << 20, controls: if prefix {1} else {1 << 20} },
                packet_cost: crate::palw_court_service_v1::ServiceCost { bytes: 1, work: 1, io: 0, controls: 1 },
                packets_per_signer: 2,
                service_span_daa: 100,
            };
            // A court service book: the shared service namespace the complete source graph is scoped to.
            let mut served = state.clone();
            {
                use crate::palw_court_service_v1::ServiceReservations;
                let service = crate::palw_court_service_state_v2::CourtServiceStateV2 {
                    version: 2,
                    source_profile_id: service_profile.id(),
                    profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(service_profile.id()),
                    book: ServiceReservations::new(service_profile.budget).unwrap(),
                    claims: Default::default(),
                };
                served.staged_courts.insert(Key::CourtService, Row::CourtService(Box::new(service).into()));
            }
            let admission_with = |capital_per_turn: u64, capital_per_node: u64, retention: u64| {
                crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1::test_only_v1(
                    service_profile,
                    crate::palw_court_service_v1::ServiceCost { bytes: 2, work: 3, io: 1, controls: 1 },
                    capital_per_turn,
                    capital_per_node,
                    1,
                    retention,
                )
            };
            let admission = admission_with(0, 0, 1);
            let budgets = (
                crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3 { max_original_leaves: 1_000_000, max_roots: 1_000_000, max_paths: 10_000_000, max_edges: 10_000_000, max_path_bytes: 10_000_000 },
                crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 { max_original_leaves: 1_000_000, max_total_edges: 10_000_000, max_total_coordinates: 1_000_000, max_root_memberships: 1_000_000 },
                crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3 { max_queries: 100_000, max_roles: 8, max_vertices: 2_000_000, max_edges: 8_000_000 },
            );
            let all_roles = PALW_FUNDED_ROLE_WHOLE_BRANCH_V1 | PALW_FUNDED_ROLE_SERVICE_OPERATOR_V1
                | PALW_FUNDED_ROLE_FEE_CAPITAL_BODIES_V1 | PALW_FUNDED_ROLE_SEMANTIC_PROVIDER_V1;
            let profiles = [h(0xFEE), h(0xCA9), h(0x9E7), h(0x5E7)];
            let bare = |roles| PalwFundedOriginalScheduleV1::test_only_v1(profiles, 700, 300, budgets.0, budgets.1, budgets.2, roles);
            let schedule = |roles| bare(roles).with_admission_v1(admission);
            // The native graph identity the payer must sign: derived here exactly as the coordinator derives it.
            let mut probe_extras = extras.clone();
            probe_extras.tir_public_originals.insert(claim, envelope.clone());
            let (graph_scope, material_until, deposit) = {
                let b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &probe_extras).unwrap();
                let prepared = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).unwrap().unwrap();
                let material = prepared.frame_v1().plan_v1().terms.material_until_daa.max(hard);
                let req = prepared.enrich_native_geometry_v1(&b, &ctx).unwrap().prepare_requirements_v1(&b, &ctx,
                    PreparedRequirementBudgetsV1 { numerical: budgets.0, input: budgets.1, source: budgets.2 }).unwrap();
                let deposit = req.frame_v1().plan_v1().terms.plan.deposit_sompi;
                (req.source_graph_v1().unwrap().scope_v3(), material, deposit)
            };
            let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x7A; 32]);
            let payer_pub = payer.verification_key.as_ref().to_vec();
            let refund = kaspa_hashes::blake2b_512_address_payload(&payer_pub);
            // `edit` changes the terms BEFORE the payer signs (the payer really signed them); `stale` changes them AFTER (a signature
            // over other terms). Construction and pairing refusals surface as `Err` (the carrier is then never a funded pairing).
            let carrier_with = |amount: u64,
                                sign_amount: u64,
                                edit: &dyn Fn(&mut FundingTermsV1),
                                stale: &dyn Fn(&mut FundingTermsV1)|
             -> Result<(Transaction, PalwFundedOriginalPairingV1), String> {
                let mut terms = FundingTermsV1 {
                    version: 1,
                    network_domain: extras.tir_public_network_domain_v1.unwrap(),
                    claim,
                    job_identity: envelope.pledge.job_identity,
                    execution_root: fp.commitment.execution_root,
                    source_id: envelope.pledge.source_id,
                    relative_graph_id: graph_scope,
                    fee_profile_id: profiles[0],
                    capital_profile_id: profiles[1],
                    retention_profile_id: profiles[2],
                    settlement_policy_id: profiles[3],
                    accept_from_daa: ctx.daa_score,
                    accept_until_daa: ctx.daa_score + 10,
                    material_until_daa: material_until,
                    funding_output_index: 1,
                    funding_amount_sompi: sign_amount,
                    payer_key_id: crate::mldsa87_primitives::mldsa87_key_id(&payer_pub),
                    refund_payload: refund,
                    nonce: h(0x0CE),
                };
                edit(&mut terms);
                let signature = libcrux_ml_dsa::ml_dsa_87::sign(&payer.signing_key, terms.id_v1().as_byte_slice(),
                    COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1, [0x7B; 32]).unwrap().as_ref().to_vec();
                stale(&mut terms);
                let tfo1 = crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1::new_v1(
                    envelope.encode_v1().unwrap(), FundingConsentV1 { terms, signature }, payer_pub.clone())?;
                let tx = Transaction::new(
                    0,
                    vec![TransactionInput::new(TransactionOutpoint::new(h(0xF00D), 0), vec![], 0, 0)],
                    vec![
                        TransactionOutput::new(1, ScriptPublicKey::default()),
                        TransactionOutput::new(amount, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&refund.as_bytes())),
                    ],
                    0,
                    crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
                    0,
                    tfo1.encode_v1()?,
                );
                let pairing = palw_funded_original_pairing_v1(&tx)?;
                Ok((tx, pairing))
            };
            let carrier_of = |amount: u64, sign_amount: u64| carrier_with(amount, sign_amount, &|_| {}, &|_| {}).unwrap();
            let (_carrier, pairing) = carrier_of(1_000, 1_000);
            let with = |sched: Option<PalwFundedOriginalScheduleV1>, pairing: &PalwFundedOriginalPairingV1| {
                let mut e = probe_extras.clone();
                e.tir_funded_originals.insert(claim, pairing.clone());
                e.funded_original_test_schedule_v1 = sched;
                e
            };
            let run = |st: &PalwChainStateV2, e: &PalwTransitionExtrasV1| {
                let b = TransitionBuilder::try_new(st, &params, false, false, false, false, e).unwrap();
                let Some(prepared) = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).ok().flatten() else {
                    return Err(FundedOriginalRefusalV1::Invalid("native preparation refused"));
                };
                coordinate_funded_original_v1(&b, &ctx, claim, prepared)
            };

            // 1. Production: no schedule value can exist → typed-missing Schedule; nothing is written.
            let prod = with(None, &pairing);
            assert!(matches!(run(&served, &prod), Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::Schedule))));
            {
                let b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &prod).unwrap();
                let before = b.state.state_root();
                let prepared = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).unwrap();
                assert!(admit_original_v1(&b, &ctx, claim, prepared).is_err());
                assert_eq!(b.state.state_root(), before);
                assert!(b.state.funded_escrows_v1().is_empty());
                // A funded pairing never rides in as an unfunded ordinary TFP1 claim.
                assert!(admit_original_v1(&b, &ctx, claim, None).is_err());
            }
            // 2. Schedule present but a role contract not adopted → that typed reason.
            assert!(matches!(run(&served, &with(Some(schedule(0)), &pairing)),
                Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::CanonicalWholeBranchActorAssignment))));
            // 3. No service book → typed-missing SharedServiceNamespace.
            assert!(matches!(run(&state, &with(Some(schedule(all_roles)), &pairing)),
                Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::SharedServiceNamespace))));
            // 4. Success: every value present → the row binds the actual outpoint, amount, payer and derived hard deadline.
            let ok = with(Some(schedule(all_roles)), &pairing);
            let plan = run(&served, &ok).unwrap();
            let row = *plan.row_v1();
            assert_eq!(row.claim, claim);
            assert_eq!(row.funding_outpoint, pairing.outpoint());
            assert_eq!(row.amount_sompi, 1_000);
            assert_eq!(row.refund_payload, refund);
            assert_eq!(row.accepted_daa, ctx.daa_score);
            assert_eq!(row.hard_deadline_daa, hard);
            assert_eq!(row.contract_id, pairing.envelope().funding.terms.id_v1());
            // Lane r5: the liability opens at admission.
            assert!(row.liability_open && row.settled_by.is_none());
            // Lane r5: branch count matches the native derivation: liable signers × turns + every source-graph node + retention.
            let derivation = plan.derivation_v1().clone();
            let liable = 1u64; // the producer only (`liable_signers_v1` at admission)
            let nodes = derivation.source_packets.len() as u64;
            assert!(nodes > 0);
            assert_eq!(derivation.signer_packets.len() as u64, liable * service_profile.packets_per_signer);
            assert_eq!(derivation.branch_count(), liable * service_profile.packets_per_signer + nodes + 1);
            assert_eq!(derivation.resources.heap_bytes, liable * service_profile.packets_per_signer + nodes);
            drop(plan);
// Within existing genuine signed native funded-original fixture, before writes.
{
    let native=TransitionBuilder::try_new(&served,&params,false,false,false,false,&ok).unwrap();
    let before=native.state.state_root();let entries=native.entries.len();
    let make=|budget:PreparedRequirementBudgetsV1| -> Result<NativePreparedArchiveAdjunctQuoteV11,ArchiveErrorV11<String>> {
        let prepared=prepare_paired_original_v1(&native,&ctx,&object,Some(&fp.commitment)).map_err(archive_state_error_v11)?.unwrap();
        let requirements=prepared.enrich_native_geometry_v1(&native,&ctx).map_err(archive_geometry_error_v11)?.prepare_requirements_v1(&native,&ctx,budget).map_err(archive_requirements_error_v11)?;
        // No install, funding receipt, Claim or service binding exists here.
        assert!(native.state.claim(&claim).is_none());assert!(native.state.funded_escrows.is_empty());
        // A real positive boundary prevents an unsupported typed inventory
        // from masquerading as the intended zero-capacity reservation refusal.
        let scratch=crate::palw_native_resource_v1::native_typed_value_requests_v1(&object).unwrap()
            .checked_add(crate::palw_native_resource_v1::native_typed_value_requests_v1(&envelope).unwrap()).unwrap();
        assert!(scratch>0);
        let exact=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:scratch,database_bytes:0});
        let exact_shape=requirements.quote_archive_object_shape_with_checker_v11(&envelope,&object,&exact).unwrap();
        assert_eq!(exact.outstanding(),(0,0),"successful read-only quote releases its actual scratch ticket");
        let short=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:scratch-1,database_bytes:0});
        assert!(matches!(requirements.quote_archive_object_shape_with_checker_v11(&envelope,&object,&short),Err(ArchiveErrorV11::Resource(_))));
        assert_eq!(short.outstanding(),(0,0));
        let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:0,database_bytes:0});
        assert!(matches!(requirements.quote_archive_object_shape_with_checker_v11(&envelope,&object,&checker),Err(ArchiveErrorV11::Resource(_))));
        // Decoder would reject this malformed payload, but the SAME production
        // inner path must report real zero-capacity refusal BEFORE decoding.
        let mut unreadable=envelope.clone();unreadable.original_payload.clear();
        assert!(matches!(requirements.quote_archive_object_shape_with_checker_v11(&unreadable,&object,&checker),Err(ArchiveErrorV11::Resource(_))));
        assert_eq!(checker.outstanding(),(0,0));
        let shape=requirements.quote_archive_object_shape_v11(&envelope,&object)?;
        assert_eq!(exact_shape,shape,"explicit checker uses the same semantic production quote");
        let (_,control_bytes,claim_bound,escrow_bound)=shape;
        assert_eq!(control_bytes,borsh::object_length(&object).unwrap() as u64);
        assert_eq!(claim_bound,652);assert_eq!(escrow_bound,borsh::object_length(&row).unwrap() as u64);
        let result=requirements.quote_archive_adjuncts_v11(&native,&ctx,&object)?;
        // An actual signed/prospective extractor with another class is no quote.
        let mut changed=object.clone();if let PalwConsensusObjectV2::FreePromptCommitted{class_id,..}=&mut changed{*class_id=h(0xAB11);}
        assert!(matches!(requirements.quote_archive_object_shape_v11(&envelope,&changed),Err(ArchiveErrorV11::Decode(_))));
        let mut changed_ids=object.clone();if let PalwConsensusObjectV2::FreePromptCommitted{prompt_token_ids,..}=&mut changed_ids{prompt_token_ids.push(0);}
        assert!(matches!(requirements.quote_archive_object_shape_v11(&envelope,&changed_ids),Err(ArchiveErrorV11::Decode(_))));
        let mut other=ctx;other.daa_score+=1;
        assert!(matches!(requirements.quote_archive_adjuncts_v11(&native,&other,&object),Err(ArchiveErrorV11::Decode(_))));
        Ok(result)
    };
    let budget=PreparedRequirementBudgetsV1{numerical:budgets.0,input:budgets.1,source:budgets.2};
    let quote=make(budget).unwrap();
    assert_eq!((quote.claim(),quote.source(),quote.resource_plan()),(claim,envelope.pledge.source_id,envelope.pledge.resource_plan_id));
    assert_eq!(quote.bodies().iter().map(|f|f.tag).collect::<Vec<_>>(),vec![3,8,9,13,14,15,16,17,19,11,12]);
    assert!(quote.additional_body_bytes_vs_v10_r4().unwrap()>0);assert!(quote.native_requests_max()>0);
    assert!(quote.metadata_work().hashed_canonical_bytes>0);assert_eq!(quote.metadata_work().pledge_signature_checks,0);
    // Compare measured baseline to actual existing native complete derivation,
    // not a supplied count or a fabricated future book.
    let ProspectiveBaseServiceV11::Measured(service)=quote.base_service() else{panic!("real native fixture has adopted admission schedule")};
    assert_eq!(service.packet_count,derivation.branch_count());
    assert_eq!(service.packets_codec_bytes,4+service.packet_count*borsh::object_length(&derivation.retention_packet).unwrap() as u64);
    assert!(quote.missing().any(|m|m==ArchiveAdjunctMissingV11::NativeFamilyServicePackets));
    // Genuine native preparation quota refusal remains Resource, before any
    // payment/claim mutation. No injected verdict or success flag is used.
    let mut tiny=budget;tiny.input.max_original_leaves=0;
    assert!(matches!(make(tiny),Err(ArchiveErrorV11::Resource(_))));
    assert_eq!(native.state.state_root(),before);assert_eq!(native.entries.len(),entries);
}

            // Lane r5: the all-branch candidate reserves every branch's packet in ONE book row (pure; the fold writes it at install).
            if batch || production {
                let b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &ok).unwrap();
                let prepared = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).unwrap().unwrap();
                let geometry = prepared.enrich_native_geometry_v1(&b, &ctx).unwrap();
                let req = geometry.prepare_requirements_v1(&b, &ctx,
                    PreparedRequirementBudgetsV1 { numerical: budgets.0, input: budgets.1, source: budgets.2 }).unwrap();
                let roots = &req.inputs_v1().parameters_v1().roots;
                let streaming_budget = crate::palw_tir_public_source_v1::parameter_cover_v1::StreamingParameterCoverBudgetV1 {
                    total: budgets.1,
                    per_root_edges: roots.iter().map(|root| root.explored_edges).max().unwrap_or(1).max(1),
                    per_root_coordinates: roots.iter().map(|root| root.explored_coordinates).max().unwrap_or(1).max(1),
                    per_root_memberships: roots.iter().map(|root| root.parameters.len() as u64).max().unwrap_or(1).max(1),
                    max_workspace_bytes: 128 << 20,
                };
                let streamed_envelope = req.geometry.streaming_batch_envelope_v1(&b, &ctx, streaming_budget, 4_096).unwrap();
                assert!(req.geometry.streaming_batch_envelope_v1(
                    &b, &ctx, streaming_budget, streamed_envelope.maximums_v1().len() - 1,
                ).is_err());
                let envelope = req.numerical_batch_envelope_v1(&b, &ctx, 4_096).unwrap();
                assert!(req.constraint_stop_batch_operands_v1(&b, &ctx).unwrap().is_none(),
                    "this numerical fixture has no verified tokenizer registration");
                assert_eq!(envelope.scope_v1(), req.source_graph_v1().unwrap().absolute_scope_v3());
                assert_eq!(streamed_envelope.scope_v1(), envelope.scope_v1());
                assert_eq!(streamed_envelope.maximums_v1(), envelope.maximums_v1());
                assert!(!envelope.maximums_v1().is_empty());
                assert_eq!(envelope.demand_ranges_v1().flatten().copied().collect::<Vec<_>>(), envelope.maximums_v1());
                assert!(req.numerical_batch_envelope_v1(&b, &ctx, envelope.maximums_v1().len() - 1).is_err());
                let class = req.frame_v1().original_v1().job.class_id;
                let program = crate::palw_native_resource_v1::decode_native_tir_program_v1(
                    b.state.tir_class_v1(&class).unwrap().program.as_slice(),
                ).unwrap();
                let profile = crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(
                    b.state.court_service_state_v1().unwrap().profile_id,
                );
                use crate::palw_court_shared_material_intake_v1::batch::native_ranges::{
                    numerical_batch_root_demand_v1, numerical_batch_streaming_demand_v1,
                };
                // Prospective geometry requires no old allocation, custody
                // pledge or accepted scope. Bind only after the native frame
                // exists; the legacy scope and positional bounds must agree.
                {
                    use crate::palw_court_shared_material_intake_v1::batch::native_ranges::{
                        numerical_batch_projection_v1, NumericalBatchProjectionV1,
                    };
                    let frame = req.frame_v1();
                    let source = req.geometry.source_v1();
                    let prompt = frame.plan_v1().owner_work.rules.prompt_form;
                    let project = |claim, form| numerical_batch_projection_v1(
                        claim, form, source, b.state.tir_class_v1(&class).unwrap(),
                        crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, class).unwrap(),
                        streaming_budget, 4_096,
                    ).unwrap();
                    let projected = project(claim, prompt);
                    assert_eq!(projected.maximums_v1(), envelope.maximums_v1());
                    let rebound = NumericalBatchProjectionV1::bind_v1(projected, frame, source, profile).unwrap();
                    assert_eq!(rebound.scope_v1(), envelope.scope_v1());
                    assert_eq!(rebound.maximums_v1(), envelope.maximums_v1());
                    assert!(NumericalBatchProjectionV1::bind_v1(project(h(0xBAD), prompt), frame, source, profile).is_err());
                    let other_prompt = match prompt {
                        crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat => crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
                        crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 => crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                    };
                    assert!(NumericalBatchProjectionV1::bind_v1(project(claim, other_prompt), frame, source, profile).is_err());
                    assert!(NumericalBatchProjectionV1::bind_v1(project(claim, prompt), frame, source, Hash64::default()).is_err());
                }
                let first_root = req.inputs_v1().roots_v1()[0].leaf;
                let selected = numerical_batch_root_demand_v1(
                    req.frame_v1(), req.geometry.source_v1(), req.inputs_v1(), &program, profile, &envelope, first_root, 0,
                ).unwrap();
                let selected = selected.ranges_v1();
                let input_roots = &req.inputs_v1().parameters_v1().roots;
                let streaming = crate::palw_tir_public_source_v1::parameter_cover_v1::StreamingParameterCoverBudgetV1 {
                    total: budgets.1,
                    per_root_edges: input_roots.iter().map(|root| root.explored_edges).max().unwrap_or(1).max(1),
                    per_root_coordinates: input_roots.iter().map(|root| root.explored_coordinates).max().unwrap_or(1).max(1),
                    per_root_memberships: input_roots.iter().map(|root| root.parameters.len() as u64).max().unwrap_or(1).max(1),
                    max_workspace_bytes: 128 << 20,
                };
                let streamed = numerical_batch_streaming_demand_v1(
                    req.frame_v1(), req.geometry.source_v1(), b.state.tir_class_v1(&class).unwrap(),
                    crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, class).unwrap(),
                    &program, profile, streaming, 4_096, first_root, 0,
                ).unwrap();
                assert_eq!(streamed.ranges_v1(), selected);
                assert!(!selected.is_empty());
                assert!(selected.iter().zip(envelope.demand_ranges_v1().next().unwrap()).all(|((_, bytes), cap)| bytes <= cap));
                assert!(selected.windows(2).all(|pair| pair[0].0.id_v1(claim).unwrap() < pair[1].0.id_v1(claim).unwrap()));
                assert!(numerical_batch_root_demand_v1(
                    req.frame_v1(), req.geometry.source_v1(), req.inputs_v1(), &program, profile, &envelope, u64::MAX, 0,
                ).is_err());
                let signers = req.frame_v1().custody_v1().liable_signers_v1();
                let (after_signer, full) = funded_book_candidate_v1(&b, &req, &derivation, &admission, &signers).ok().unwrap();
                let bound = &full.claims[&claim];
                assert_eq!(bound.packet_ids, derivation.packet_ids());
                assert_eq!(bound.packet_ids.len() as u64, derivation.branch_count());
                assert_eq!(full.book.packets.len() as u64, derivation.branch_count());
                assert_eq!(after_signer.claims[&claim].packet_ids.len() as u64, liable * service_profile.packets_per_signer);
                // Feed the actual numerical input geometry into the Batch
                // scheduler while the original still has no Claim row.
                use crate::palw_court_shared_material_intake_v1::{
                    material_intake_schedule_v1,
                    batch::admission::{
                        NativeBatchDeliveryCostsV1, NativeBatchDeliveryTimingV1, batch_numerical_service_ids_v1,
                        reserve_native_batch_delivery_v1,
                    },
                };
                use crate::palw_court_service_v1::ServiceCost;
                use crate::palw_court_shared_material_intake_v1::batch::numerical_verification::NumericalBatchReplayBudgetV1;
                let (verification, terminal) = batch_numerical_service_ids_v1(claim, streamed_envelope.scope_v1()).unwrap();
                let cost = |bytes| ServiceCost { bytes, work: 1, io: 1, controls: 1 };
                let costs = NativeBatchDeliveryCostsV1 {
                    numerical_verification: NumericalBatchReplayBudgetV1::test_only_v1(),
                    terminal: NumericalBatchReplayBudgetV1::test_only_v1().service_cost_v1().unwrap(), recovery_daa: 1, maximum_packets: 32_768,
                    timing: NativeBatchDeliveryTimingV1 {
                        open_daa: 8, provider_wait_daa: 64, begin_daa: 16, parts_daa: 512,
                        finish_daa: 16, verification_daa: 16, stop_daa: 16, terminal_daa: 16,
                    },
                };
                let storage = material_intake_schedule_v1().unwrap();
                let journal = funded_journal_row_candidate_v1(&b, &req, &derivation, full.clone()).unwrap();
                let expected_edges = crate::palw_funded_original_admission_v1::funded_admission_causal_edges_v1(
                    req.source_graph_v1().unwrap(), &derivation,
                ).unwrap();
                let journal = match journal {
                    Row::CourtServiceV3(row) => (*row).clone(),
                    Row::CourtService(row) => crate::palw_court_service_release_v1::CourtServiceRowV3::from_v2(&row),
                    _ => unreachable!("funded book journal row"),
                };
                assert_eq!(journal.causal, expected_edges);
                let mut foreign = journal.clone();
                foreign.state.profile_id = h(0xB47C);
                assert!(matches!(
                    reserve_native_batch_delivery_v1(
                        foreign, req.frame_v1(), &streamed_envelope, None,
                        NativeBatchDeliveryCostsV1 { ..costs }, storage, |_, bytes| Ok(cost(bytes)),
                    ),
                    Err(crate::palw_batch_court_v1::BatchCourtErrorV1::CaseGraph),
                ));
                let (base, base_book, base_scope) = crate::palw_funded_original_admission_v1::funded_batch_base_candidate_v1(
                    &b.state, req.frame_v1(), &admission,
                ).unwrap();
                assert!(base.source_packets.is_empty());
                assert_eq!(base.signer_packets, derivation.signer_packets);
                assert_eq!(base.branch_count(), liable * service_profile.packets_per_signer + 1);
                assert_eq!(base_book.state.claims[&claim].packet_ids, base.packet_ids());
                use crate::palw_court_shared_material_intake_v1::batch::admission::{
                    NativeBatchDeliveryShapeV1, reserve_native_batch_delivery_shape_v1, compile_native_batch_delivery_shape_v1,
                };
                let shape = |scope| NativeBatchDeliveryShapeV1 {
                    claim, scope, accepted_daa: ctx.daa_score,
                    material_until_daa: req.frame_v1().plan_v1().terms.material_until_daa,
                    maximums: streamed_envelope.maximums_v1(),
                };
                // The prospective reservation owns no CourtServiceRow/claim
                // binding. It must reserve exactly what native install does.
                let compiled = compile_native_batch_delivery_shape_v1(
                    shape(h(0xB47D)), None, costs, storage, |_, bytes| Ok(cost(bytes)),
                ).unwrap();
                let relative_id = compiled.relative_graph_id_v1();
                drop(compiled);
                let mut projected_book = base_book.state.book.clone();
                let (projected_quote, projected_graph) = reserve_native_batch_delivery_shape_v1(
                    &mut projected_book, shape(streamed_envelope.scope_v1()), None, costs, storage, |_, bytes| Ok(cost(bytes)),
                ).unwrap();
                {
                    use crate::palw_court_shared_material_intake_v1::batch::admission::{
                        CompiledBatchAdmissionV1, compile_native_batch_plain_output_v2,
                    };
                    use crate::palw_court_shared_material_intake_v1::batch::plain_output::{
                        BATCH_PLAIN_OUTPUT_ORDINAL_V2, PlainBatchOutputV2,
                    };
                    use crate::palw_batch_court_v1::rooted::batch_open_packet_v1;
                    let scope = streamed_envelope.scope_v1();
                    let plan = req.frame_v1().plan_v1();
                    let profile = crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(base_book.state.profile_id);
                    let (before_frame_scope, _) = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::scope_from_native_inputs_v2(
                        req.geometry.source_v1(), claim, plan.allocation_id, plan.rules_id, profile,
                    ).unwrap();
                    assert_eq!(before_frame_scope, scope);
                    let output = PlainBatchOutputV2::derive_v2(
                        req.frame_v1().original_v1(), req.geometry.source_v1(), cost(8_192), 16,
                    ).unwrap();
                    assert!(PlainBatchOutputV2::derive_v2(
                        req.frame_v1().original_v1(), req.geometry.source_v1(), cost(1), 16,
                    ).is_err());
                    let rooted = crate::palw_court_shared_material_intake_v1::batch::plain_output::rooted_receipt_fixture_v2(
                        crate::palw_court_shared_material_intake_v1::batch::plain_output::SignedBatchPlainOutputV2 {
                            version: 2, claim, scope, source: req.geometry.source_v1().source_id_v1(),
                            notice: h(21), policy: h(22), material: h(23),
                            filer: req.frame_v1().custody_v1().pledge_v1().producer,
                            signature: [0; crate::palw_court_shared_material_intake_v1::batch::plain_output::PLAIN_OUTPUT_SIGNATURE_BYTES_V2],
                        },
                    );
                    let key = Key::CourtBatchPlainOutputReceiptV2(claim);
                    let mut generic = TransitionBuilder::try_new(&served, &params, false, false, false, false, &ok).unwrap();
                    assert!(matches!(
                        generic.write_staged_row_v1(key, Some(Row::CourtBatchPlainOutputReceiptV2(Box::new(rooted)))),
                        Err(PalwStateV2Error::CarriageInconsistent(why)) if why.contains("requires native adjudication")
                    ));
                    assert!(generic.state.staged_court_row_v1(&key).is_none());
                    let mut foreign_shape = shape(scope);
                    foreign_shape.claim = h(0xB47E);
                    assert!(compile_native_batch_plain_output_v2(
                        foreign_shape, &output, costs, storage, |_, bytes| Ok(cost(bytes)),
                    ).is_err());
                    let compile = |scope| compile_native_batch_plain_output_v2(
                        shape(scope), &output, costs, storage, |_, bytes| Ok(cost(bytes)),
                    ).unwrap();
                    assert_eq!(compile(h(0xB47D)).relative_graph_id_v1(), compile(scope).relative_graph_id_v1());
                    let mut book = base_book.state.book.clone();
                    let (output_quote, graph) = CompiledBatchAdmissionV1::reserve_v1(compile(scope), &mut book).unwrap();
                    assert_ne!(output_quote.relative_graph_id, projected_quote.relative_graph_id);
                    let output_open = batch_open_packet_v1(claim, scope, BATCH_PLAIN_OUTPUT_ORDINAL_V2).unwrap().as_bytes();
                    let output_open_at = book.assigned_daa(&output_open).unwrap();
                    let terminal_at = book.assigned_daa(&terminal).unwrap();
                    assert!(output_open_at < terminal_at);
                    assert!(graph.iter().find(|node| node.packet.id == terminal).unwrap().predecessors.len() == 2);
                    assert!(base_book.state.book.assigned_daa(&output_open).is_none());
                }
                let (quote, batch_book) = reserve_native_batch_delivery_v1(
                    base_book, req.frame_v1(), &streamed_envelope, None, costs, storage, |_, bytes| Ok(cost(bytes)),
                ).unwrap();
                assert_eq!(relative_id, quote.relative_graph_id,
                    "relative graph identity must survive final packet namespace binding");
                assert_eq!(projected_quote, quote);
                assert_eq!(projected_book, batch_book.state.book);
                assert!(projected_graph.iter().all(|node| batch_book.state.claims[&claim].packet_ids.contains(&node.packet.id)));
                let batch_packets = &batch_book.state.claims[&claim].packet_ids;
                assert!(quote.required_daa <= 10_000 && base.packet_ids().is_subset(batch_packets));
                assert!(derivation.source_packets.iter().all(|packet| !batch_packets.contains(&packet.id)));
                // The original payer consent accepted above cannot authorize
                // this additional native Batch graph.
                use crate::palw_court_original_escrow_v1::{FundingRefusalV1, OriginalFundingScopeV1, prepare_funding_v1};
                let funded = pairing.envelope();
                let t = &funded.funding.terms;
                let batch_scope = OriginalFundingScopeV1 {
                    claim, job_identity: t.job_identity, execution_root: t.execution_root, source_id: t.source_id,
                    relative_graph_id: base_scope, fee_profile_id: t.fee_profile_id,
                    capital_profile_id: t.capital_profile_id, retention_profile_id: t.retention_profile_id,
                    settlement_policy_id: t.settlement_policy_id,
                    required_amount_sompi: crate::palw_funded_original_admission_v1::funded_required_amount_v1(
                        700, 300, base.capital_sompi, deposit,
                    ).unwrap(),
                    required_material_until_daa: req.frame_v1().plan_v1().terms.material_until_daa.max(ctx.daa_score + 10_000),
                }.with_native_batch_v1(&quote).unwrap();
                assert!(matches!(prepare_funding_v1(
                    funded.inspect_carrier_v1(pairing.carrier()).unwrap(), &funded.funding, &funded.payer_pubkey,
                    &batch_scope, t.network_domain, ctx.daa_score,
                ), Err(FundingRefusalV1::Scope)));
                let amount = batch_scope.required_amount_sompi;
                let (batch_carrier, batch_pairing) = carrier_with(amount, amount, &|terms| {
                    terms.relative_graph_id = batch_scope.relative_graph_id;
                    terms.material_until_daa = batch_scope.required_material_until_daa;
                }, &|_| {}).unwrap();
                let funded_batch = batch_pairing.envelope();
                let checked = prepare_funding_v1(
                    funded_batch.inspect_carrier_v1(&batch_carrier).unwrap(), &funded_batch.funding, &funded_batch.payer_pubkey,
                    &batch_scope, t.network_domain, ctx.daa_score,
                ).unwrap();
                assert_eq!(checked.actual().outpoint(), batch_pairing.outpoint());
                let batch_schedule = schedule(all_roles).with_batch_v1(FundedBatchScheduleV1 {
                    cover: streaming_budget, maximum_ranges: 4_096, delivery: costs,
                    carrier_costs: [cost(0); 4], open_work_per_range: 0, finish_work_per_byte: 0, finish_io_per_byte: 0,
                    stop_verification: cost(1), stop_filing: cost(1),
                });
                let batch_extras = |pairing: &PalwFundedOriginalPairingV1| {
                    let mut e = with(Some(batch_schedule), pairing);
                    e.tir_public_conviction_active = true;
                    e.batch_court_active_v1 = true;
                    e.batch_numerical_profile_v1 = Some(crate::palw_court_shared_material_intake_v1::NumericalBatchReplayProfileV1 {
                        service_profile: batch_book.state.profile_id, budget: costs.numerical_verification,
                    });
                    e
                };
                let batch_params = if batch { params.clone().with_fp_quanta(8,64).unwrap() } else { params.clone() };
                let batch_params = batch_params.with_court_clock_v2_from_daa(Some(ctx.daa_score));
                let old_extras = batch_extras(&pairing);
                let mut refused = TransitionBuilder::try_new(&served, &batch_params, false, false, false, false, &old_extras).unwrap();
                let before = refused.state.state_root();
                assert!(super::super::apply_object(&mut refused, &ctx, &object).is_err());
                assert_eq!(refused.state.state_root(), before);
                let funded_extras = batch_extras(&batch_pairing);
                // The same genuinely signed FP/payer bytes wrapped with a
                // companion must not enter the legacy funded profile. The
                // opaque companion is deliberately unauthenticated here;
                // no public profile or companion acceptance is claimed.
                {
                    use crate::palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3;
                    let companion = [0u8; misaka_palw_positive_contract::public_one_honest::SignedPublicOriginalV1::WIRE_BYTES];
                    let mut paired_tx = batch_carrier.clone();
                    paired_tx.payload = PalwTirFundedOriginalEnvelopeV3::from_parts_v3(&batch_carrier.payload, b"native carrier prompt", &companion)
                        .unwrap().encode_v3().unwrap();
                    paired_tx.finalize();
                    let paired = palw_funded_original_pairing_v1(&paired_tx).unwrap();
                    assert_eq!(paired.outpoint().transaction_id, paired_tx.id());
                    let paired_extras = batch_extras(&paired);
                    let mut rejected = TransitionBuilder::try_new(&served, &batch_params, false, false, false, false, &paired_extras).unwrap();
                    let before = rejected.state.state_root();
                    let entries = rejected.entries.len();
                    assert!(matches!(prepare_paired_original_v1(&rejected, &ctx, &object, Some(&fp.commitment)),
                        Err(PalwStateV2Error::PublicOriginalProfileUnavailable { claim: refused }) if refused == claim));
                    // Removing the legacy view must not downgrade a recognized
                    // companion into an unpaired old-style commitment.
                    let mut no_legacy = paired_extras.clone();
                    no_legacy.tir_public_originals.clear();
                    let without_view = TransitionBuilder::try_new(&served, &batch_params, false, false, false, false, &no_legacy).unwrap();
                    assert!(matches!(prepare_paired_original_v1(&without_view, &ctx, &object, None),
                        Err(PalwStateV2Error::PublicOriginalProfileUnavailable { claim: refused }) if refused == claim));
                    assert!(matches!(palw_tir_public_preclaim_quote_v1(&served, &batch_params, &ctx, &paired_extras, &object,
                        paired_extras.tir_public_originals.get(&claim).unwrap()),
                        Err(PalwStateV2Error::PublicOriginalProfileUnavailable { claim: refused }) if refused == claim));
                    // The coordinator's second guard must also refuse a frame
                    // prepared under the historical profile, without installing it.
                    let legacy = TransitionBuilder::try_new(&served, &batch_params, false, false, false, false, &funded_extras).unwrap();
                    let prepared = prepare_paired_original_v1(&legacy, &ctx, &object, Some(&fp.commitment)).unwrap().unwrap();
                    assert!(matches!(coordinate_funded_original_v1(&rejected, &ctx, claim, prepared),
                        Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::PublicOriginalProfile))));
                    let why = super::super::apply_object(&mut rejected, &ctx, &object).unwrap_err();
                    assert!(matches!(why, PalwStateV2Error::PublicOriginalProfileUnavailable { claim: refused } if refused == claim), "{why:?}");
                    assert_eq!(rejected.state.state_root(), before);
                    assert_eq!(rejected.entries.len(), entries);
                    assert!(rejected.state.claim(&claim).is_none());
                }
                let mut accepted = TransitionBuilder::try_new(&served, &batch_params, false, false, false, false, &funded_extras).unwrap();
                super::super::apply_object(&mut accepted, &ctx, &object).unwrap();
                assert_eq!(accepted.state.claim(&claim).unwrap().accepted_block, ctx.block);
                let installed = crate::palw_court_shared_tir_state_v1::context(&accepted.state, claim).unwrap();
                assert!(installed.batch_notice_only_v1());
                assert_eq!(installed.hard_daa, ctx.daa_score + 10_000);
                let mut wrong = installed.clone();
                wrong.version = crate::palw_court_shared_material_intake_v1::batch::plain_output::BATCH_PLAIN_OUTPUT_CONTEXT_VERSION_V2;
                assert!(wrong.check_context(&accepted.state).is_err(), "a version byte cannot upgrade a legacy service binding");
                use crate::palw_batch_court_v1::carriage::{
                    BatchPlainOutputTerminalV2, BatchPlainOutputWireV2, encode_batch_plain_output_payload_v2,
                    quote_batch_plain_output_terminal_v2,
                };
                let terminal_wire = BatchPlainOutputWireV2::Terminal(BatchPlainOutputTerminalV2 { claim, receipt_digest: h(17) });
                let payload = encode_batch_plain_output_payload_v2(&terminal_wire).unwrap();
                let unchanged = accepted.state.state_root();
                assert!(matches!(
                    accepted.apply_batch_plain_output_payload_v2(&ctx, &payload),
                    Err(PalwStateV2Error::CarriageInconsistent(why)) if why.contains("V2-capable original")
                ));
                assert!(quote_batch_plain_output_terminal_v2(&accepted.state, claim).is_err());
                assert_eq!(accepted.state.state_root(), unchanged);
                assert_eq!(accepted.state.court_service_row_v3_v1(), Some(&batch_book));
                assert_eq!(accepted.state.funded_escrows_v1()[&checked.consent().terms.id_v1()].funding_outpoint, batch_pairing.outpoint());
                assert_eq!(quote.retained_payload_bytes, envelope.maximums_v1().iter().sum::<u64>());
                assert!(batch_book.state.claims[&claim].packet_ids.contains(&terminal));
                assert_eq!(batch_book.causal[&terminal], [verification]);
                assert_eq!(batch_book.causal[&verification].len(), envelope.maximums_v1().len());
                let book = &batch_book.state.book;
                assert!(book.assigned_daa(&terminal).unwrap() > book.assigned_daa(&verification).unwrap());
                // The served book itself is untouched by the dry run.
                assert!(b.state.court_service_state_v1().unwrap().claims.is_empty());
            }
            // Lane r5: each missing piece refuses with no write.
            // (a) no admission values → typed-missing AdmissionSchedule.
            assert!(matches!(run(&served, &with(Some(bare(all_roles)), &pairing)),
                Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::AdmissionSchedule))));
            // (b) court capacity cannot hold every branch (retention bytes beyond the per-DAA budget) → refused whole.
            let huge = bare(all_roles).with_admission_v1(admission_with(0, 0, 1 << 41));
            assert!(run(&served, &with(Some(huge), &pairing)).is_err());
            // (c) a book opened under another profile → refused (never re-profiled).
            {
                let mut other_book = served.clone();
                let mut service = other_book.court_service_state_v1().unwrap().clone();
                service.source_profile_id = h(0x5E5);
                service.profile_id = crate::palw_court_service_state_v2::court_service_profile_id_v2(h(0x5E5));
                other_book.staged_courts.insert(Key::CourtService, Row::CourtService(Box::new(service).into()));
                assert!(run(&other_book, &ok).is_err());
            }
            // (d) a liable actor without current authority (bond no longer active) → refused.
            {
                let mut exiting = served.clone();
                let bond = exiting.bonds.get_mut(&PalwBondKeyV2(fp.commitment.job.executor_bond)).unwrap();
                bond.status = PalwBondStatusV2::Retiring { since_daa: 0, settled_at_since: 0 };
                assert!(run(&exiting, &ok).is_err());
            }
            // (e) capital: per-branch capital raises the required escrow; the signed 1,000 no longer covers it.
            let costly = bare(all_roles).with_admission_v1(admission_with(10_000, 10_000, 1));
            assert!(matches!(run(&served, &with(Some(costly), &pairing)), Err(FundedOriginalRefusalV1::Invalid(_))));
            assert!(served.funded_escrows_v1().is_empty(), "a refused coordination writes nothing");
            // 5. Funding below the required amount is refused, even though the payer signed and the output matches its terms.
            let required = crate::palw_funded_original_admission_v1::funded_required_amount_v1(700, 300, 0, deposit).unwrap();
            let (_, short) = carrier_of(required - 1, required - 1);
            assert!(matches!(run(&served, &with(Some(schedule(all_roles)), &short)), Err(FundedOriginalRefusalV1::Invalid(_))));
            // 6. A schedule profile the payer did not sign (caller profile hash is not policy) → refused by the payer-consent check.
            // fix-funded: the schedule must carry the admission values, or the earlier typed-missing `AdmissionSchedule` answers
            // first and the profile comparison is never reached (the original reproducer's schedule had none).
            let consent_refused = |r: Result<_, FundedOriginalRefusalV1>| {
                matches!(r, Err(FundedOriginalRefusalV1::Invalid(why)) if why == "payer consent does not cover the native scope, amount, horizon or interval")
            };
            for slot in 0..4 {
                let mut unsigned = profiles;
                unsigned[slot] = h(0x1_000 + slot as u64);
                let other = PalwFundedOriginalScheduleV1::test_only_v1(unsigned, 700, 300, budgets.0, budgets.1, budgets.2, all_roles)
                    .with_admission_v1(admission);
                assert!(consent_refused(run(&served, &with(Some(other), &pairing))), "unsigned schedule profile slot {slot} is refused");
                // At the seam: the typed refusal writes nothing (no FP, frame or escrow delta); the carrier stays ordinary and its
                // funding output stays the payer's own P2PKH fallback.
                let other_extras = with(Some(other), &pairing);
                let b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &other_extras).unwrap();
                let before = b.state.state_root();
                let prepared = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).unwrap();
                assert!(admit_original_v1(&b, &ctx, claim, prepared).is_err());
                assert_eq!(b.state.state_root(), before);
                assert!(b.state.funded_escrows_v1().is_empty() && b.state.claim(&claim).is_none());
                let out = &pairing.carrier().outputs[pairing.outpoint().index as usize];
                assert_eq!(crate::mldsa87_primitives::p2pkh_mldsa87_payload(&out.script_public_key), Some(refund));
            }
            // 6b. fix-funded: every signed term is bound. (i) Terms the coordinator re-derives natively (network, source, graph scope,
            // the four profiles, material horizon, acceptance interval): the payer validly signed a different value, the TFO1 pairs,
            // and the coordinator refuses it.
            assert!(ctx.daa_score >= 2);
            let rederived: Vec<(&str, Box<dyn Fn(&mut FundingTermsV1)>)> = vec![
                ("network_domain", Box::new(|t| t.network_domain = h(0xD0))),
                ("source_id", Box::new(|t| t.source_id = h(0xD1))),
                ("relative_graph_id", Box::new(|t| t.relative_graph_id = h(0xD2))),
                ("fee_profile_id", Box::new(|t| t.fee_profile_id = h(0xD3))),
                ("capital_profile_id", Box::new(|t| t.capital_profile_id = h(0xD4))),
                ("retention_profile_id", Box::new(|t| t.retention_profile_id = h(0xD5))),
                ("settlement_policy_id", Box::new(|t| t.settlement_policy_id = h(0xD6))),
                ("material_until_daa short", Box::new(|t| t.material_until_daa = material_until - 1)),
                ("acceptance not yet open", Box::new(|t| t.accept_from_daa = ctx.daa_score + 1)),
                ("acceptance already closed", Box::new(|t| {
                    t.accept_from_daa = ctx.daa_score - 2;
                    t.accept_until_daa = ctx.daa_score - 1;
                })),
            ];
            for (field, edit) in &rederived {
                let (_, signed) = carrier_with(1_000, 1_000, edit.as_ref(), &|_| {}).unwrap_or_else(|e| panic!("{field}: {e}"));
                assert!(consent_refused(run(&served, &with(Some(schedule(all_roles)), &signed))), "{field}: refused");
            }
            // (ii) Terms fixed by the literal original or the actual output (claim, job identity, execution root, payer key, refund
            // payload, funding output index and amount): the TFO1 is not constructible or does not pair with its own carrier, so it
            // never reaches the coordinator as a funded pairing.
            let unpairable: Vec<(&str, u64, Box<dyn Fn(&mut FundingTermsV1)>)> = vec![
                ("claim", 1_000, Box::new(|t| t.claim = h(0xE0))),
                ("job_identity", 1_000, Box::new(|t| t.job_identity = h(0xE1))),
                ("execution_root", 1_000, Box::new(|t| t.execution_root = h(0xE2))),
                ("payer_key_id", 1_000, Box::new(|t| t.payer_key_id = h(0xE3))),
                ("refund_payload", 1_000, Box::new(|t| t.refund_payload = h(0xE4))),
                ("funding_output_index other output", 1_000, Box::new(|t| t.funding_output_index = 0)),
                ("funding_output_index absent", 1_000, Box::new(|t| t.funding_output_index = 2)),
                ("funding_amount below actual", 999, Box::new(|_| {})),
                ("funding_amount above actual", 1_001, Box::new(|_| {})),
            ];
            for (field, signed_amount, edit) in &unpairable {
                assert!(carrier_with(1_000, *signed_amount, edit.as_ref(), &|_| {}).is_err(), "{field}: never a funded pairing");
            }
            // (iii) A signature over other terms (terms changed after signing; scope untouched) → refused by the payer signature.
            let (_, tampered) = carrier_with(1_000, 1_000, &|_| {}, &|t| t.nonce = h(0xE5)).unwrap();
            assert!(consent_refused(run(&served, &with(Some(schedule(all_roles)), &tampered))));
            assert!(served.funded_escrows_v1().is_empty(), "a refused coordination writes nothing");
            // 7. The current producer authority changed after signing → refused by the actor catalog.
            {
                let mut rotated = served.clone();
                rotated.bonds.get_mut(&PalwBondKeyV2(fp.commitment.job.executor_bond)).unwrap().operator_id = h(0xBAD);
                assert!(run(&rotated, &ok).is_err());
            }
            // 8. Atomicity: the escrow row installed inside a checkpoint vanishes with every other write when the object fails.
            {
                let mut b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &ok).unwrap();
                let before = b.state.state_root();
                let entries = b.entries.len();
                let saved = b.try_checkpoint().unwrap();
                b.install_funded_escrow_v1(row).unwrap();
                assert_eq!(b.state.funded_escrows_v1().len(), 1);
                b.restore(saved);
                assert_eq!(b.state.state_root(), before);
                assert_eq!(b.entries.len(), entries);
                assert!(b.state.funded_escrows_v1().is_empty());
            }
        }
