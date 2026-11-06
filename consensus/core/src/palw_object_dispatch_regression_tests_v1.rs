// Private regression fixtures for the mechanical object dispatcher split.
// These exercise existing production apply_object without production rule changes.
// The lifetime fixture assumes prior authenticated rows and does not cover
// native ingress or accepted history.

#[test]
fn paired_fp_late_original_deposit_failure_restores_the_entire_checkpoint() {
    use crate::palw_freeprompt_v3::{PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;
    let seed = [0x6D; 32]; // Public deterministic TEST key, never network funds.
    let kp = mldsa::generate_key_pair(seed);
    let key = kp.verification_key.as_ref().to_vec();
    // The key participates in fp_job_id. Re-execute the existing tiny
    // fixture in the changed job's OWN context; never transplant old roots.
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |job| job.executor_pubkey = key.clone(), None);
    let (world, params, mut extras) = world(&fx, &fx.frame);
    let params = params.with_fp_quanta(1, 64).unwrap().with_fp_exposure_ceiling(1).unwrap();
    // Explicit tiny fixture policy, not a production preset change.
    let mut globals = crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = 1;
    extras.model_registry = Some(crate::palw_model_registry_v1::PalwModelRegistryFoldV1 {
        globals,
        span_daa: 600,
        genesis_works: Default::default(),
        grace_until_daa: 0,
        admission_audit_period_daa: None,
        readiness_v2_active: false,
        bond_maturity: None,
    });
    let mut state = PalwChainStateV2::genesis();
    state.bonds = world.bonds.clone();
    state.classes = world.classes.clone();
    state.class_shares = world.class_shares.clone();
    state.tir_classes = world.tir_classes.clone();
    let class = fx.frame.original_v1().job.class_id;
    state.staged_courts.insert(Key::TirLayout(class), world.staged_courts[&Key::TirLayout(class)].clone());
    let mut fp = PalwFpCommitmentTxPayloadV3 {
        version: PALW_FP_V3_VERSION,
        commitment: fx.frame.original_v1().clone(),
        prompt_token_ids: fx.execution.prompt.clone(),
        signature: vec![0; SIG],
    };
    assert_eq!(fp.commitment.job.executor_pubkey, key);
    assert_eq!(
        crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment),
        crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&fx.frame.header_v1().job_context)
    );
    let claim = fp.claim_id();
    fp.signature =
        mldsa::sign(&kp.signing_key, claim.as_byte_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, seed).unwrap().as_ref().to_vec();
    let producer = PalwBondKeyV2(fp.commitment.job.executor_bond);
    let bond = state.bonds.get_mut(&producer).unwrap();
    bond.pubkey = key.clone();
    bond.operator_id = palw_operator_id_v2(&key);
    bond.payout_payload = kaspa_hashes::blake2b_512_address_payload(&key);
    let mut object = fx.object.clone();
    if let PalwConsensusObjectV2::FreePromptCommitted { claim: c, public_job, executor_pubkey, job_pin, .. } = &mut object {
        *c = claim;
        *public_job = Some(Box::new(fp.commitment.clone()));
        *executor_pubkey = key.clone();
        *job_pin = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
    } else {
        panic!("original fixture object");
    }
    let ctx = PalwBlockContextV2 { block: h(0xF135), daa_score: fx.frame.plan_v1().terms.accepted_daa, blue_score: 1, subsidy: 0 };
    let builder = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    let (plan, _, _, _) = original_plan_v1(&builder, ctx.daa_score, &fp.commitment, fx.frame.header_v1()).unwrap();
    let mut pledge = fx.frame.custody_v1().pledge_v1().clone();
    pledge.claim = claim;
    pledge.job_identity = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
    pledge.resource_plan_id = plan.resource_plan_id;
    pledge.source_id = plan.source_id;
    pledge.signature = mldsa::sign(
        &kp.signing_key,
        pledge.id_v1().as_byte_slice(),
        crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1,
        seed,
    )
    .unwrap()
    .as_ref()
    .to_vec();
    fp.validate_signature_v3(crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1).unwrap();
    let envelope = crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1::new_v1(
        borsh::to_vec(&fp).unwrap(),
        fx.frame.header_v1().clone(),
        pledge.clone(),
        plan.terms.plan.rent_sompi,
    )
    .unwrap();
    // Reuse the real original preparation, including producer pledge, source,
    // registered geometry and semantic bundle. This does not install a claim.
    prepare_original_v1(&builder, &ctx, &object, &fp.commitment, &envelope.header, &pledge).unwrap();

    // Run the actual ordinary admission first. This proves the object can pass
    // every ordinary check and measures its real reservation, rather than
    // guessing a price or injecting a synthetic post-write error.
    drop(builder);
    let mut ordinary = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    apply_object(&mut ordinary, &ctx, &object).unwrap();
    let accepted = ordinary.state.claim(&claim).unwrap();
    let commitment = palw_claim_commitment_v1(&params, accepted, ctx.daa_score).unwrap();
    assert!(commitment > 0);
    assert!(ordinary.state.reserved_exposure(&producer) >= commitment);
    state.bonds.get_mut(&producer).unwrap().collateral = u64::try_from(commitment.checked_mul(1000).unwrap()).unwrap();
    // At 1 permille this leaves exactly the ordinary commitment's room.
    let mut limited_ordinary = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    apply_object(&mut limited_ordinary, &ctx, &object).unwrap();
    assert_eq!(limited_ordinary.gate_room(&producer, ctx.daa_score, PalwRcoreGateV1::Work).unwrap(), 0);
    assert!(limited_ordinary.state.claim(&claim).is_some());
    assert!(!limited_ordinary.entries.is_empty());
    assert!(plan.terms.plan.deposit_sompi > 0);

    // The ordinary controls use the historical unpaired lane; the actual
    // paired failure below also arms G03, so its mandatory frame is present.
    drop(ordinary);
    drop(limited_ordinary);
    extras.tir_public_conviction_active = true;
    extras.tir_public_originals.insert(claim, envelope);
    let mut paired = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    let root_before = paired.state.state_root();
    let carriage_before = borsh::to_vec(&PalwStateCarriageV2::from_state(&paired.state)).unwrap();
    let journal_before = borsh::to_vec(&paired.entries).unwrap();
    let reservations_before = paired.state.reserved_exposure.clone();
    let work_ids_before = paired.state.work_ids.clone();
    let error = apply_object(&mut paired, &ctx, &object).unwrap_err();
    // This named error is reached only by install_original_v1 AFTER the
    // ordinary reserve_for_claim, write_claim and public-job/deadline writes.
    assert!(
        matches!(error, PalwStateV2Error::CarriageInconsistent(ref why)
        if why == "TIR original full deposit exceeds producer native work room"),
        "{error:?}"
    );
    assert_eq!(paired.state.state_root(), root_before);
    assert_eq!(borsh::to_vec(&PalwStateCarriageV2::from_state(&paired.state)).unwrap(), carriage_before);
    assert_eq!(borsh::to_vec(&paired.entries).unwrap(), journal_before);
    assert_eq!(paired.state.reserved_exposure, reservations_before);
    assert_eq!(paired.state.work_ids, work_ids_before);
    assert!(paired.state.claim(&claim).is_none());
    // Native resource spent-work counters are deliberately NOT monetary
    // reservations and must not be refunded by this checkpoint assertion.
}

// The authorization/capture/enrollment rows below represent previously
// authenticated records, just as the existing fold fixtures do. This is a
// focused fold-unit fixture, not a complete native accepted-history fixture:
// no actual permissionless engine draw/authorization ingress is claimed. It
// covers common guard consumption and real receipt-handler writes only;
// public activation remains refused.
fn dispatcher_lifetime_fixture(
    service_delta: u64,
    pause: Option<(u64, u64)>,
) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwConsensusObjectV2, u64) {
    use crate::palw_panel_lifetime_v2::*;
    use crate::palw_permissionless_panel_v1::{PalwPanelV3ParamsV1, PanelPolicyV1};
    let fx = palw_tir_public_frame_fixture_v1(None, 2);
    let (mut state, params, extras) = world(&fx, &fx.frame);
    let claim_id = fx.frame.plan_v1().original_claim;
    let accepted = state.claims[&claim_id].accepted_daa;
    let network = h(0x4E7);
    let ruleset = h(0x4E8);
    let policy = PanelPolicyV1 {
        seal_depth_blocks: 1,
        seal_wait_daa: 50,
        bond_maturity_daa: 1,
        beacon_period_daa: 10,
        beacon_wait_daa: 2,
        assignment_delay_daa: 1,
        receipt_window_daa: 40,
        seat_count: 3,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 100,
        max_candidates: 32,
        max_pending: 8,
        max_pending_per_bond: 4,
        max_assignments_per_block: 8,
        max_admissions_per_block: 4,
        max_tracked_claims: 100,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 100,
        beacon_scheme: h(777),
    };
    policy.validate().unwrap();
    let lifetime_policy = PalwPanelLifetimeFenceV2::unapproved_probe_at(crate::config::params::ForkActivation::new(0)).policy;
    let params = params
        .with_panel_v3(Some(PalwPanelV3ParamsV1 { wire_version: 2, from_daa: 0, policy, network, ruleset }))
        .with_panel_lifetime_v2(Some(PalwPanelLifetimeMirrorV2 { from_daa: 0, policy: lifetime_policy }));
    // This is an old ordinary receipt fixture: no prospective original frame
    // may grant qualified public-TIR credit in this test.
    state.staged_courts.remove(&fx.frame.key_v1());
    super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
    let claim = state.claims.get_mut(&claim_id).unwrap();
    claim.phase = PalwClaimPhaseV2::PanelBound { bound_daa: accepted };
    claim.rcore = PalwClaimRcoreV1::default();
    claim.work_id = Some(h(0x4E9));
    let work = claim.work_id.unwrap();
    let producer = claim.bond;
    let claim = claim.clone();
    let admitted = crate::palw_permissionless_panel_v1::panel_admitted_claim_v1(&state, claim_id, 1).unwrap();
    let terms = PanelLifetimeAuthorizationV2 {
        version: 2,
        network: network.as_bytes(),
        ruleset: ruleset.as_bytes(),
        nonce: h(0x4EA).as_bytes(),
        producer: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&producer),
        class: claim.class_id.as_bytes(),
        accept_from_daa: accepted,
        accept_by_daa: accepted,
        service_until_daa: accepted + service_delta,
        liability_until_daa: accepted + service_delta + 2 * params.window_court() + params.window_challenge(),
        maximum_retries: 1,
        maximum_pause_daa: pause.map_or(10, |(_, maximum)| maximum),
        maximum_pwu: claim.pwu,
        maximum_reserved_sompi: claim.reserved,
        maximum_escrow_sompi: claim.escrowed_reward,
        maximum_rights_sompi: claim.rights_reserved,
    };
    terms.validate().unwrap();
    let capture = PalwPanelConsentCaptureV2 {
        authorization: terms.id(),
        claim: claim_id,
        accepted_daa: accepted,
        immutable_fields: admitted.immutable_fields,
    };
    let enrollment = PalwPanelLifetimeEnrollmentV2 {
        terms: PanelConsumerLifetimeTermsV2 {
            version: 2,
            authorization: terms.id(),
            network: terms.network,
            ruleset: terms.ruleset,
            claim: claim_id.as_bytes(),
            class: terms.class,
            work: work.as_bytes(),
            producer: terms.producer,
            immutable_fields: capture.immutable_fields.as_bytes(),
            accept_by_daa: accepted,
            service_until_daa: terms.service_until_daa,
            liability_until_daa: terms.liability_until_daa,
            maximum_retries: terms.maximum_retries,
            maximum_pause_daa: terms.maximum_pause_daa,
        },
        signer: producer,
        enrolled_daa: accepted,
        enrolled_block: claim.accepted_block,
        policy: lifetime_policy.id(),
        pause_started_daa: pause.map(|(start, _)| accepted + start),
        total_paused_daa: 0,
        authority_kind: 0,
    };
    enrollment.terms.validate().unwrap();
    let authorization = PalwPanelAuthorizationRowV2 {
        terms,
        signer: producer,
        authorized_daa: accepted,
        authorized_block: h(0x4EC),
        policy: lifetime_policy.id(),
        consumed_claim: Some(claim_id),
    };
    let mut route = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(network, ruleset),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: accepted, burned: 0 },
    );
    for (key, bytes) in [
        (panel_authorization_key_v2(&terms.id()), borsh::to_vec(&authorization).unwrap()),
        (panel_capture_key_v2(&claim_id), borsh::to_vec(&capture).unwrap()),
        (borsh::to_vec(&claim_id.as_bytes()).unwrap(), borsh::to_vec(&enrollment).unwrap()),
    ] {
        route.aux.insert((PALW_PANEL_LIFETIME_TABLE_V2, key), bytes);
    }
    state.kernel_route = Some(route);
    panel_lifetime_claim_v2(&state, &params, &claim_id, state.claim(&claim_id).unwrap()).unwrap().unwrap();
    let receipts = drawn()
        .into_iter()
        .enumerate()
        .map(|(i, seat)| {
            use crate::palw_panel_v2::*;
            use libcrux_ml_dsa::ml_dsa_87 as mldsa;
            let seed = [u8::try_from(i + 10).unwrap(); 32]; // Public test-only keys.
            let kp = mldsa::generate_key_pair(seed);
            let key = kp.verification_key.as_ref().to_vec();
            let bond = state.bonds.get_mut(&seat).unwrap();
            bond.pubkey = key.clone();
            bond.operator_id = palw_operator_id_v2(&key);
            bond.payout_payload = kaspa_hashes::blake2b_512_address_payload(&key);
            let message = palw_receipt_message_v2(network, claim_id, PalwReceiptVerdictV2::Valid, accepted + 1);
            let signature = mldsa::sign(&kp.signing_key, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT, seed)
                .unwrap()
                .as_ref()
                .to_vec();
            assert!(crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                &key,
                message.as_byte_slice(),
                &signature,
                PALW_RECEIPT_V2_MLDSA87_CONTEXT
            ));
            PalwSeatReceiptV2 {
                claim: claim_id,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: seat,
                signed_daa: accepted + 1,
                signature,
            }
        })
        .collect();
    state
        .panel_duties
        .insert(claim_id, PalwPanelDutyRowV1 { seats: drawn().into_iter().map(|seat| (seat, 0)).collect(), seat_exposure: 0 });
    (state, params, extras, PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }, accepted)
}

fn assert_dispatch_lifetime_boundary(service_delta: u64, pause: Option<(u64, u64)>, boundary_delta: u64) {
    let (state, params, extras, object, accepted) = dispatcher_lifetime_fixture(service_delta, pause);
    let PalwConsensusObjectV2::ReceiptLicensed { claim, .. } = &object else { unreachable!() };
    let ctx = PalwBlockContextV2 { block: h(0x4ED), daa_score: accepted + boundary_delta, blue_score: 1, subsidy: 0 };
    let mut refused = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    let before = borsh::to_vec(&PalwStateCarriageV2::from_state(&refused.state)).unwrap();
    let root = refused.state.state_root();
    let reservations = refused.state.reserved_exposure.clone();
    let error = apply_object(&mut refused, &PalwBlockContextV2 { daa_score: ctx.daa_score + 1, ..ctx }, &object).unwrap_err();
    assert!(
        matches!(error, PalwStateV2Error::KernelRouteRefused(ref why)
        if why == "Panel receipt inclusion exceeds its signed absolute service or pause budget"),
        "{error:?}"
    );
    assert_eq!(refused.state.state_root(), root);
    assert_eq!(borsh::to_vec(&PalwStateCarriageV2::from_state(&refused.state)).unwrap(), before);
    assert_eq!(refused.state.reserved_exposure, reservations);
    assert!(refused.entries.is_empty());

    let mut boundary = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    apply_object(&mut boundary, &ctx, &object).unwrap();
    assert!(matches!(boundary.state.claim(claim).unwrap().phase,
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } if licensed_daa == ctx.daa_score));
    assert!(boundary.state.panel_duties[claim].seats.values().all(|credited| *credited == ctx.daa_score));
    assert!(!boundary.entries.is_empty());
    assert_ne!(boundary.state.state_root(), root);
}

#[test]
fn object_dispatch_panel_lifetime_service_deadline_is_inclusive_and_late_receipts_write_nothing() {
    assert_dispatch_lifetime_boundary(5, None, 5);
}
#[test]
fn object_dispatch_panel_lifetime_maximum_pause_is_inclusive_and_over_budget_receipts_write_nothing() {
    assert_dispatch_lifetime_boundary(20, Some((3, 2)), 5);
}
