struct PreparationSourcesFixtureV2 {
    fx: PalwTirPublicFrameFixtureV1,
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    extras: PalwTransitionExtrasV1,
    ctx: PalwBlockContextV2,
    object: PalwConsensusObjectV2,
    fp: crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3,
    envelope: crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1,
}
fn preparation_sources_fixture_v2() -> PreparationSourcesFixtureV2 {
    use crate::palw_freeprompt_v3::{PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;
    let seed = [0x6D; 32]; // Public deterministic TEST key, never network funds.
    let kp = mldsa::generate_key_pair(seed);
    let key = kp.verification_key.as_ref().to_vec();
    // The key participates in fp_job_id. Re-execute the existing tiny
    // fixture in the changed job's OWN context; never transplant old roots.
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |job| job.executor_pubkey = key.clone(), None);
    let (world, params, mut extras) = world(&fx, &fx.frame);
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
    drop(builder);
    PreparationSourcesFixtureV2 { fx, state, params, extras, ctx, object, fp, envelope }
}
fn preparation_sources_child_v2(name: &str, mode: u8) {
    const CHILD: &str = "MISAKA_PREPARATION_SOURCES_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        preparation_sources_worker_v2(mode);
    } else {
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
}
#[test]
fn preparation_sources_owned_actual_paired_and_quote_earliest_allocation_quota_retry() {
    preparation_sources_child_v2("preparation_sources_owned_actual_paired_and_quote_earliest_allocation_quota_retry", 0)
}
#[test]
fn preparation_sources_owned_actual_frame_allocation_refusal_and_second_signature_lifetime() {
    preparation_sources_child_v2("preparation_sources_owned_actual_frame_allocation_refusal_and_second_signature_lifetime", 1)
}
fn preparation_sources_worker_v2(mode: u8) {
    use crate::palw_native_resource_v1::{
        NativeDecodeErrorV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
    };
    use crate::palw_tir_public_allocation_v1::{
        allocation_source_stages_v2 as allocation_stages, reset_allocation_source_stages_v2 as reset_allocation,
    };
    use crate::palw_tir_public_claim_v1::{preparation_source_stages_v2 as stages, reset_preparation_source_stages_v2 as reset};
    let f = preparation_sources_fixture_v2();
    let mut extras = f.extras.clone();
    extras.tir_public_originals.insert(f.fp.claim_id(), f.envelope.clone());
    // The existing real signed funded-candidate fixture is fresh and unclaimed.
    let b = TransitionBuilder::try_new(&f.state, &f.params, false, false, false, false, &extras).unwrap();
    let (plan, native, upper, geometry) = original_plan_v1(&b, f.ctx.daa_score, &f.fp.commitment, &f.envelope.header).unwrap();
    let producer = b.state.bond(&geometry.producer).unwrap();
    let panel = std::collections::BTreeSet::new();
    let custody = crate::palw_tir_public_custody_v1::PalwTirPublicCustodyContractV1::new_v1(
        f.envelope.pledge.clone(),
        plan.terms,
        panel.clone(),
        plan.source_id,
        plan.resource_plan_id,
        upper,
        &producer.pubkey,
        crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
    )
    .unwrap();
    let verify = crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1;
    let make_frame = |verify: &dyn Fn(&[u8], &[u8], &[u8], &[u8]) -> bool| {
        Frame::prepare_native_source_v2(
            f.fp.commitment.clone(),
            f.envelope.header.clone(),
            plan.clone(),
            custody.clone(),
            &geometry,
            &f.fx.record,
            &f.fx.layout,
            None,
            None,
            native,
            &panel,
            &f.object,
            verify,
        )
    };
    let expected = make_frame(&verify).unwrap();
    reset();
    let paired = prepare_paired_original_v1(&b, &f.ctx, &f.object, Some(&f.fp.commitment)).unwrap().unwrap();
    assert_eq!(paired.frame_v1(), &expected);
    assert_eq!(stages(), [1, 1, 1, 1]);
    assert_eq!(palw_tir_public_preclaim_quote_v1(&f.state, &f.params, &f.ctx, &extras, &f.object, &f.envelope).unwrap(), plan);
    drop(paired);
    drop(b);
    const LOCAL_BYTES: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LOCAL_BYTES, database_bytes: 0 }));
    if mode == 0 {
        let run = |available: u64| {
            let blocker = reserve_native_requests_v1(LOCAL_BYTES - available).unwrap();
            reset();
            reset_allocation();
            let result = match TransitionBuilder::try_new(&f.state, &f.params, false, false, false, false, &extras) {
                Err(e) => Err(e),
                Ok(b) => {
                    let before = b.state.state_root();
                    let entries = b.entries.len();
                    let result = prepare_paired_original_v1(&b, &f.ctx, &f.object, Some(&f.fp.commitment));
                    assert_eq!(b.state.state_root(), before);
                    assert_eq!(b.entries.len(), entries);
                    result.map(|p| {
                        assert_eq!(p.unwrap().frame_v1(), &expected);
                    })
                }
            };
            let observed = (stages(), allocation_stages());
            drop(blocker);
            (result, observed)
        };
        let (ok, observed) = run(LOCAL_BYTES);
        ok.unwrap();
        assert_eq!(observed, ([1, 1, 1, 1], [2, 2, 2]));
        let (mut lo, mut hi) = (0, LOCAL_BYTES);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (_, s) = run(mid);
            if s.1[1] > 0 { hi = mid } else { lo = mid + 1 }
        }
        assert!(lo > 0);
        let (err, s) = run(lo - 1);
        assert_eq!(
            s,
            ([0; 4], [1, 0, 0]),
            "target earliest native allocation Source, not earlier builder rejection or later direct constructors"
        );
        assert!(matches!(err, Err(PalwStateV2Error::NativeResource(_))));
        let (ok, s) = run(LOCAL_BYTES);
        ok.unwrap();
        assert_eq!(s, ([1, 1, 1, 1], [2, 2, 2]));
        // Quote is an actual caller too, and must preserve the same resulting plan.
        assert_eq!(palw_tir_public_preclaim_quote_v1(&f.state, &f.params, &f.ctx, &extras, &f.object, &f.envelope).unwrap(), plan);
    } else {
        let q = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
            &f.fx.record,
            &f.fx.layout,
            &f.envelope.header,
        )
        .unwrap();
        assert!(q > 0 && q < LOCAL_BYTES);
        let blocker = reserve_native_requests_v1(LOCAL_BYTES - (q - 1)).unwrap();
        reset();
        reset_allocation();
        let called = std::cell::Cell::new(0);
        let actual = |pk: &[u8], message: &[u8], sig: &[u8], ctx: &[u8]| {
            called.set(called.get() + 1);
            verify(pk, message, sig, ctx)
        };
        assert!(matches!(make_frame(&actual), Err(NativeDecodeErrorV1::Resource(_))));
        assert_eq!(stages(), [0; 4]);
        assert_eq!(
            allocation_stages(),
            [1, 0, 0],
            "Frame API refused in its earlier native derivation, not second direct constructor"
        );
        assert_eq!(called.get(), 0);
        drop(blocker);
        let blocker = reserve_native_requests_v1(LOCAL_BYTES - q).unwrap();
        reset();
        let actual = |pk: &[u8], message: &[u8], sig: &[u8], ctx: &[u8]| {
            // This is the actual signature verifier called while the retained
            // Source is borrowed, not a replacement authentication decision.
            assert!(reserve_native_requests_v1(1).is_err());
            called.set(called.get() + 1);
            verify(pk, message, sig, ctx)
        };
        assert_eq!(make_frame(&actual).unwrap(), expected);
        assert!(called.get() > 0);
        assert_eq!(stages(), [0, 0, 1, 1]);
        // Constructor owner is gone at return, while the returned Frame remains.
        let recovered = reserve_native_requests_v1(q).unwrap();
        drop(recovered);
        drop(blocker);
        let mut wrong = f.envelope.header.clone();
        wrong.full_logits_trace_root = Hash64::default();
        reset();
        assert!(matches!(
            Frame::prepare_native_source_v2(
                f.fp.commitment.clone(),
                wrong,
                plan.clone(),
                custody.clone(),
                &geometry,
                &f.fx.record,
                &f.fx.layout,
                None,
                None,
                native,
                &panel,
                &f.object,
                verify
            ),
            Err(NativeDecodeErrorV1::Decode(_))
        ));
        assert_eq!(make_frame(&verify).unwrap(), expected);
    }
    assert!(reserve_native_requests_v1(LOCAL_BYTES).is_ok(), "Source and builder reservations released");
}
