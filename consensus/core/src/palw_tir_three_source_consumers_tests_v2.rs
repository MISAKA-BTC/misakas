// Actual native entrypoint controls over an accepted-original fixture. Ordinary
// FP payment/admission is assumed by the fixture, not claimed as mined coverage.
fn source_three_sign(key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair, message: Hash64, context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message.as_byte_slice(), context, [41; 32]).unwrap().as_ref().to_vec()
}
fn source_three_control(
    frame: &Frame,
    actor: PalwBondKeyV2,
    target: T,
    action: A,
    rent: u64,
    key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    network: Hash64,
) -> Control {
    let mut control = Control {
        version: 1,
        claim: frame.plan_v1().original_claim,
        allocation_id: frame.plan_v1().allocation_id,
        source_id: frame.plan_v1().source_id,
        target,
        expected_turn: 0,
        declared_rent_sompi: rent,
        signer: actor,
        action,
        signature: vec![],
    };
    control.signature = source_three_sign(
        key,
        control.signing_message_v1(network).unwrap(),
        crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1,
    );
    control
}
fn source_three_descriptor(frame: &Frame) -> crate::palw_backend::PalwTirPublicPreparedDescriptorV1 {
    let source = frame.header_v1();
    crate::palw_backend::PalwTirPublicPreparedDescriptorV1 {
        version: 1,
        source_id: frame.plan_v1().source_id,
        class_id: frame.descriptor_v1().class_id,
        artifact_root: source.artifact_root,
        execution_root: source.committed_execution_root,
        trace_root: source.full_logits_trace_root,
        step_root: source.step_merkle_root,
        step_leaf_count: source.step_leaf_count,
        decode_count: source.job_context.exact_decode_tokens,
        vocab: frame.plan_v1().acquisition.envelope.vocab,
        material_until_daa: frame.plan_v1().terms.material_until_daa,
        manifest_digest: h(0xD1),
        retained_bytes: 1,
    }
}
struct SourceThreeFixture {
    fx: PalwTirPublicFrameFixtureV1,
    producer_key: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    seat_key: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    owner_key: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    extras: PalwTransitionExtrasV1,
}
fn source_three_fixture() -> SourceThreeFixture {
    use crate::palw_tir_public_custody_v1::{PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1, PalwTirPublicCustodyContractV1};
    let producer_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([191; 32]);
    let seat_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([192; 32]);
    let owner_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([193; 32]);
    // Key is part of the actual job before context, execution and claim IDs
    // are derived; no later key rotation or mismatched original is accepted.
    let mut fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(
        4,
        |job| job.executor_pubkey = producer_key.verification_key.as_ref().to_vec(),
        None,
    );
    let original = fx.frame.original_v1().clone();
    let plan = fx.frame.plan_v1().clone();
    let mut pledge = fx.frame.custody_v1().pledge_v1().clone();
    pledge.signature = source_three_sign(&producer_key, pledge.id_v1(), PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1);
    let verify = crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1;
    let custody = PalwTirPublicCustodyContractV1::new_v1(
        pledge,
        plan.terms,
        std::collections::BTreeSet::new(),
        plan.source_id,
        plan.resource_plan_id,
        fx.rules.maximum_custodians,
        producer_key.verification_key.as_ref(),
        verify,
    )
    .unwrap();
    if let PalwConsensusObjectV2::FreePromptCommitted { executor_pubkey, .. } = &mut fx.object {
        *executor_pubkey = producer_key.verification_key.as_ref().to_vec();
    } else {
        panic!("FP fixture")
    }
    let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
        class_id: original.job.class_id,
        producer: PalwBondKeyV2(original.job.executor_bond),
        execution_root: original.execution_root,
        job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&original),
        accepted_daa: plan.terms.accepted_daa,
    };
    fx.frame = Frame::prepare_native_v1(
        original,
        fx.frame.header_v1().clone(),
        plan,
        custody,
        &geometry,
        &fx.record,
        &fx.layout,
        None,
        None,
        fx.rules,
        &std::collections::BTreeSet::new(),
        &fx.object,
        verify,
    )
    .unwrap();
    let (mut state, params, extras) = world(&fx, &fx.frame);
    state.bonds.get_mut(&fx.frame.custody_v1().pledge_v1().producer).unwrap().pubkey = producer_key.verification_key.as_ref().to_vec();
    state.bonds.get_mut(&honest_seat()).unwrap().pubkey = seat_key.verification_key.as_ref().to_vec();
    state.bonds.get_mut(&outsider()).unwrap().pubkey = owner_key.verification_key.as_ref().to_vec();
    state.staged_courts.remove(&fx.frame.key_v1());
    super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
    SourceThreeFixture { fx, producer_key, seat_key, owner_key, state, params, extras }
}
fn source_three_installed(f: &SourceThreeFixture) -> PalwChainStateV2 {
    let mut b = TransitionBuilder::try_new(&f.state, &f.params, false, false, false, false, &f.extras).unwrap();
    install_original_v1(&mut b, &at(&f.fx.frame, 0), f.fx.frame.clone(), &prompt_row(&f.fx, &f.fx.frame)).unwrap();
    bind_panel_v1(&mut b, &at(&f.fx.frame, 0), f.fx.frame.plan_v1().original_claim).unwrap();
    // world() supplies the accepted claim and actual Panel, but intentionally
    // omits its economic duty row. Use the same native priced duty writer as
    // binding, rather than fabricating a credited receipt, lock or Ready row.
    let claim_id = f.fx.frame.plan_v1().original_claim;
    let claim = b.state.claim(&claim_id).unwrap().clone();
    let panel = b.state.panel(&claim_id).unwrap().clone();
    b.reserve_seat_duties(claim_id, &claim, &panel.seats, panel.bound_daa).unwrap();
    assert_eq!(b.state.panel_duties_of(&claim_id).unwrap().get(&honest_seat()), Some(&0));
    b.state.clone()
}
fn source_three_grant_input(
    f: &SourceThreeFixture,
) -> (PalwChainStateV2, crate::palw_tir_public_carriage_v1::PalwTirPublicGrantReadyV1) {
    use crate::palw_panel_v2::{PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2};
    use crate::palw_tir_public_custody_v1::{PALW_TIR_SOURCE_GRANT_CONTEXT_V1, PalwTirPublicCustodyGrantV1};
    use crate::palw_tir_public_owned_v1::{
        PalwTirPublicOwnedActionV1 as Owned, PalwTirPublicWorkDebitV1, PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1,
        PalwTirPublicWorkV1,
    };
    let installed = source_three_installed(f);
    let frame = &f.fx.frame;
    let p = frame.plan_v1();
    let network = f.extras.tir_public_network_domain_v1.unwrap();
    let debit = PalwTirPublicWorkDebitV1 {
        expected_nonce: 0,
        previous_head: Hash64::default(),
        scope: PalwTirPublicWorkScopeV1 {
            claim: p.original_claim,
            owner: frame.custody_v1().pledge_v1().producer,
            reserved_daa: p.terms.accepted_daa,
            bundle_id: p.allocation_id,
            source_id: p.source_id,
            turn: 0,
            request: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 0 },
            actor: honest_seat(),
            phase: PalwTirPublicWorkPhaseV1::Capture,
            input_head: p.source_id,
            input_offset: 0,
            slice: 0,
            cursor_id: h(9),
        },
        work: PalwTirPublicWorkV1 { attempts: 1, parser_steps: 1, ..Default::default() },
        restart: false,
        history: None,
    };
    let capture = source_three_control(
        frame,
        honest_seat(),
        T::Preparation { original_accepted_daa: p.terms.accepted_daa },
        A::Preparation(Owned::WorkDebit(debit)),
        0,
        &f.seat_key,
        network,
    );
    let mut b = TransitionBuilder::try_new(&installed, &f.params, false, false, false, false, &f.extras).unwrap();
    apply_control_v1(&mut b, &at(frame, 4), network, &capture).unwrap();
    let signed = at(frame, 5).daa_score;
    let mut receipt = PalwSeatReceiptV2 {
        claim: p.original_claim,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: honest_seat(),
        signed_daa: signed,
        signature: vec![],
    };
    receipt.signature = source_three_sign(
        &f.seat_key,
        palw_receipt_message_v2(network, receipt.claim, receipt.verdict, signed),
        PALW_RECEIPT_V2_MLDSA87_CONTEXT,
    );
    let mut grant = PalwTirPublicCustodyGrantV1 {
        version: 1,
        claim: p.original_claim,
        seat: honest_seat(),
        signed_daa: signed,
        pledge_id: frame.custody_v1().pledge_v1().id_v1(),
        valid_receipt_digest: PalwTirPublicCustodyGrantV1::receipt_digest_v1(&receipt),
        signature: vec![],
    };
    grant.signature = source_three_sign(&f.seat_key, grant.id_v1(), PALW_TIR_SOURCE_GRANT_CONTEXT_V1);
    let mut movement = crate::palw_tir_public_carriage_v1::PalwTirPublicGrantReadyV1 {
        version: 1,
        claim: p.original_claim,
        allocation_id: p.allocation_id,
        source_id: p.source_id,
        signer: honest_seat(),
        receipt,
        grant,
        descriptor: source_three_descriptor(frame),
        signature: vec![],
    };
    movement.signature = source_three_sign(
        &f.seat_key,
        movement.signing_message_v1(network).unwrap(),
        crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1,
    );
    (b.state.clone(), movement)
}
#[test]
fn three_source_consumers_actual_grant_triple_signatures_positive_before_quota() {
    let f = source_three_fixture();
    let (state, movement) = source_three_grant_input(&f);
    let mut b = TransitionBuilder::try_new(&state, &f.params, false, false, false, false, &f.extras).unwrap();
    super::output_source_owned_v2::reset_lifecycle_v2();
    apply_grant_ready_v1(&mut b, &at(&f.fx.frame, 5), f.extras.tir_public_network_domain_v1.unwrap(), &movement).unwrap();
    assert_eq!(super::output_source_owned_v2::lifecycle_v2(), (1, 1, 1));
    let frame = palw_tir_public_claim_v1(&b.state, movement.claim).unwrap();
    assert!(frame.original_custody_v1().credit_v1(&honest_seat()).is_some());
    assert_eq!(b.state.panel_duties_of(&movement.claim).unwrap().get(&honest_seat()), Some(&at(&f.fx.frame, 5).daa_score));
    assert!(b.state.slashable_lock(honest_seat(), movement.claim).unwrap().amount > 0);
    // An independently re-signed descriptor for a different execution remains
    // semantic even though all three signature envelopes are real and valid.
    let mut wrong = movement.clone();
    wrong.descriptor.execution_root = Hash64::default();
    wrong.signature = source_three_sign(
        &f.seat_key,
        wrong.signing_message_v1(f.extras.tir_public_network_domain_v1.unwrap()).unwrap(),
        crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_GRANT_READY_CONTEXT_V1,
    );
    let mut negative = TransitionBuilder::try_new(&state, &f.params, false, false, false, false, &f.extras).unwrap();
    let original_root = negative.state.state_root();
    super::output_source_owned_v2::reset_lifecycle_v2();
    assert!(matches!(
        apply_grant_ready_v1(&mut negative, &at(&f.fx.frame, 5), f.extras.tir_public_network_domain_v1.unwrap(), &wrong),
        Err(PalwStateV2Error::CarriageInconsistent(_))
    ));
    assert_eq!(negative.state.state_root(), original_root);
    assert!(negative.entries.is_empty());
    assert_eq!(super::output_source_owned_v2::lifecycle_v2(), (1, 1, 0));
    let before = b.state.state_root();
    assert!(apply_grant_ready_v1(&mut b, &at(&f.fx.frame, 5), f.extras.tir_public_network_domain_v1.unwrap(), &movement).is_err());
    assert_eq!(b.state.state_root(), before);
}
fn source_three_capture(f: &SourceThreeFixture, actor: PalwBondKeyV2, key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair) -> Control {
    use crate::palw_tir_public_owned_v1::{
        PalwTirPublicOwnedActionV1 as Owned, PalwTirPublicWorkDebitV1, PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1,
        PalwTirPublicWorkV1,
    };
    let frame = &f.fx.frame;
    let p = frame.plan_v1();
    let debit = PalwTirPublicWorkDebitV1 {
        expected_nonce: 0,
        previous_head: Hash64::default(),
        scope: PalwTirPublicWorkScopeV1 {
            claim: p.original_claim,
            owner: frame.custody_v1().pledge_v1().producer,
            reserved_daa: p.terms.accepted_daa,
            bundle_id: p.allocation_id,
            source_id: p.source_id,
            turn: 0,
            request: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 0 },
            actor,
            phase: PalwTirPublicWorkPhaseV1::Capture,
            input_head: p.source_id,
            input_offset: 0,
            slice: 0,
            cursor_id: h(9),
        },
        work: PalwTirPublicWorkV1 { attempts: 1, parser_steps: 1, ..Default::default() },
        restart: false,
        history: None,
    };
    source_three_control(
        frame,
        actor,
        T::Preparation { original_accepted_daa: p.terms.accepted_daa },
        A::Preparation(Owned::WorkDebit(debit)),
        0,
        key,
        f.extras.tir_public_network_domain_v1.unwrap(),
    )
}
fn source_three_ready(f: &SourceThreeFixture) -> PalwChainStateV2 {
    let installed = source_three_installed(f);
    let frame = &f.fx.frame;
    let actor = frame.custody_v1().pledge_v1().producer;
    let network = f.extras.tir_public_network_domain_v1.unwrap();
    let mut b = TransitionBuilder::try_new(&installed, &f.params, false, false, false, false, &f.extras).unwrap();
    apply_control_v1(&mut b, &at(frame, 1), network, &source_three_capture(f, actor, &f.producer_key)).unwrap();
    let ready = source_three_control(
        frame,
        actor,
        T::Preparation { original_accepted_daa: frame.plan_v1().terms.accepted_daa },
        A::Preparation(crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1::PreparationReady {
            descriptor: source_three_descriptor(frame),
        }),
        0,
        &f.producer_key,
        network,
    );
    apply_control_v1(&mut b, &at(frame, 2), network, &ready).unwrap();
    b.state.clone()
}
fn source_three_quota_entry(mode: u8) {
    use super::output_source_owned_v2::{lifecycle_v2, observed_v2, reset_lifecycle_v2, reset_observed_v2};
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1};
    let f = source_three_fixture();
    let frame = &f.fx.frame;
    let network = f.extras.tir_public_network_domain_v1.unwrap();
    let (state, grant) = if mode == 2 {
        let (s, g) = source_three_grant_input(&f);
        (s, Some(g))
    } else if mode == 1 {
        (source_three_ready(&f), None)
    } else {
        (f.state.clone(), None)
    };
    let open_offset = frame.plan_v1().preparation_until_daa - frame.plan_v1().terms.accepted_daa;
    let open = source_three_control(
        frame,
        outsider(),
        T::OwnerOpen { owner: outsider() },
        A::Open,
        owner_terms_v1(frame).unwrap().plan.rent_sompi,
        &f.owner_key,
        network,
    );
    let dispatch = |b: &mut TransitionBuilder<'_>| match mode {
        0 => install_original_v1(b, &at(frame, 0), frame.clone(), &prompt_row(&f.fx, frame)),
        1 => apply_control_v1(b, &at(frame, open_offset), network, &open),
        _ => apply_grant_ready_v1(b, &at(frame, 5), network, grant.as_ref().unwrap()),
    };
    // Establish the actual positive entrypoint before arming any quota.
    let mut positive = TransitionBuilder::try_new(&state, &f.params, false, false, false, false, &f.extras).unwrap();
    reset_lifecycle_v2();
    dispatch(&mut positive).unwrap();
    assert_eq!(lifecycle_v2(), (1, 1, 1));
    drop(positive);
    const LOCAL_BYTES: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LOCAL_BYTES, database_bytes: 0 }));
    let run = |available: u64| {
        let block = reserve_native_requests_v1(LOCAL_BYTES - available).unwrap();
        reset_observed_v2();
        reset_lifecycle_v2();
        let mut b = match TransitionBuilder::try_new(&state, &f.params, false, false, false, false, &f.extras) {
            Ok(b) => b,
            Err(e) => {
                drop(block);
                return (Err(e), (0, 0), (0, 0, 0), true);
            }
        };
        let before = b.state.state_root();
        let entries = b.entries.len();
        let result = dispatch(&mut b);
        let unchanged = result.is_ok() || (b.state.state_root() == before && b.entries.len() == entries);
        let observed = observed_v2();
        let lifecycle = lifecycle_v2();
        drop(b);
        drop(block);
        (result, observed, lifecycle, unchanged)
    };
    let (result, _, lifecycle, _) = run(LOCAL_BYTES);
    result.unwrap();
    assert_eq!(lifecycle, (1, 1, 1));
    let (mut low, mut high) = (0, LOCAL_BYTES);
    while low < high {
        let middle = low + (high - low) / 2;
        let (_, observed, _, _) = run(middle);
        if observed.1 > 0 { high = middle } else { low = middle + 1 }
    }
    assert!(low > 0);
    let (result, observed, lifecycle, unchanged) = run(low - 1);
    assert_eq!(
        observed,
        (1, 0),
        "must enter this actual consumer's Source constructor; earlier validation/checkpoint is not Source admission evidence"
    );
    assert_eq!(lifecycle, (0, 0, 0));
    assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
    assert!(unchanged, "actual entrypoint restores state+journal on Source resource refusal");
    let (result, _, lifecycle, _) = run(LOCAL_BYTES);
    result.unwrap();
    assert_eq!(lifecycle, (1, 1, 1), "same actual input retries with owner live through final borrow then dropped");
    assert!(reserve_native_requests_v1(LOCAL_BYTES).is_ok(), "entrypoint releases its actual Source/builder tickets after operation");
}
fn source_three_child(name: &str, mode: u8) {
    const CHILD: &str = "MISAKA_THREE_SOURCE_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        source_three_quota_entry(mode);
        return;
    }
    let full = format!("{}::{name}", module_path!());
    assert!(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
            .env(CHILD, name)
            .status()
            .unwrap()
            .success()
    );
}
#[test]
fn three_source_consumers_plain_install_source_stage_quota_rollback_retry() {
    source_three_child("three_source_consumers_plain_install_source_stage_quota_rollback_retry", 0)
}
#[test]
fn three_source_consumers_signed_owner_open_source_stage_quota_rollback_retry() {
    source_three_child("three_source_consumers_signed_owner_open_source_stage_quota_rollback_retry", 1)
}
#[test]
fn three_source_consumers_signed_grant_ready_source_stage_quota_rollback_retry() {
    source_three_child("three_source_consumers_signed_grant_ready_source_stage_quota_rollback_retry", 2)
}
