    /// Explicit artifact generator; never enables fixture constructors in production.
    #[test]
    fn g14_native_funded_candidate_fixture_generator_v1() {
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;
        use crate::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PALW_FP_V3_VERSION, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT};
        let seed = [0x6D; 32]; // Public deterministic TEST key, never network funds.
        let kp = mldsa::generate_key_pair(seed);
        let key = kp.verification_key.as_ref().to_vec();
        // The key participates in fp_job_id. Re-execute the existing tiny
        // fixture in the changed job's OWN context; never transplant old roots.
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(
            2, |job| job.executor_pubkey = key.clone(), None,
        );
        let (world, params, mut extras) = world(&fx, &fx.frame);
        // Explicit tiny fixture policy, not a production preset change.
        let mut globals = crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;
        globals.seat_count = 1;
        extras.model_registry = Some(crate::palw_model_registry_v1::PalwModelRegistryFoldV1 {
            globals, span_daa: 600, genesis_works: Default::default(), grace_until_daa: 0,
            admission_audit_period_daa: None, readiness_v2_active: false, bond_maturity: None,
        });
        let mut state = PalwChainStateV2::genesis();
        state.bonds = world.bonds.clone();
        state.classes = world.classes.clone();
        state.class_shares = world.class_shares.clone();
        state.tir_classes = world.tir_classes.clone();
        let class = fx.frame.original_v1().job.class_id;
        state.staged_courts.insert(Key::TirLayout(class), world.staged_courts[&Key::TirLayout(class)].clone());
        let mut fp = PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION, commitment: fx.frame.original_v1().clone(),
            prompt_token_ids: fx.execution.prompt.clone(), signature: vec![0; SIG],
        };
        assert_eq!(fp.commitment.job.executor_pubkey, key);
        assert_eq!(crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment),
            crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&fx.frame.header_v1().job_context));
        let claim = fp.claim_id();
        fp.signature = mldsa::sign(&kp.signing_key, claim.as_byte_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, seed).unwrap().as_ref().to_vec();
        let producer = PalwBondKeyV2(fp.commitment.job.executor_bond);
        let bond = state.bonds.get_mut(&producer).unwrap();
        bond.pubkey = key.clone();
        bond.operator_id = palw_operator_id_v2(&key);
        bond.payout_payload = kaspa_hashes::blake2b_512_address_payload(&key);
        let mut object = fx.object.clone();
        if let PalwConsensusObjectV2::FreePromptCommitted { claim:c, public_job, executor_pubkey, job_pin, .. } = &mut object {
            *c = claim;
            *public_job = Some(Box::new(fp.commitment.clone()));
            *executor_pubkey = key.clone();
            *job_pin = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
        } else { panic!("original fixture object"); }
        let ctx = PalwBlockContextV2 { block: h(0xF135), daa_score: fx.frame.plan_v1().terms.accepted_daa, blue_score: 1, subsidy: 0 };
        let builder = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
        let (plan, _, _, _) = original_plan_v1(&builder, ctx.daa_score, &fp.commitment, fx.frame.header_v1()).unwrap();
        let mut pledge = fx.frame.custody_v1().pledge_v1().clone();
        pledge.claim = claim;
        pledge.job_identity = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
        pledge.resource_plan_id = plan.resource_plan_id;
        pledge.source_id = plan.source_id;
        pledge.signature = mldsa::sign(&kp.signing_key, pledge.id_v1().as_byte_slice(),
            crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1, seed).unwrap().as_ref().to_vec();
        fp.validate_signature_v3(crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1).unwrap();
        let envelope = crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1::new_v1(
            borsh::to_vec(&fp).unwrap(), fx.frame.header_v1().clone(), pledge.clone(), plan.terms.plan.rent_sompi).unwrap();
        // Reuse the real original preparation, including producer pledge, source,
        // registered geometry and semantic bundle. This does not install a claim.
        prepare_original_v1(&builder, &ctx, &object, &fp.commitment, &envelope.header, &pledge).unwrap();
        assert!(state.claim(&claim).is_none());
        let carriage = PalwStateCarriageV2::from_state(&state);
        let loaded = carriage.clone().into_state(&params, Some(state.state_root())).unwrap();
        assert_eq!(loaded.state_root(), state.state_root());
        if let Ok(dir) = std::env::var("MISAKA_G14_NATIVE_FIXTURE_DIR") {
            let dir = std::path::Path::new(&dir);
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("state.borsh"), borsh::to_vec(&carriage).unwrap()).unwrap();
            std::fs::write(dir.join("params.borsh"), borsh::to_vec(&params).unwrap()).unwrap();
            std::fs::write(dir.join("original.tfp1"), envelope.encode_v1().unwrap()).unwrap();
            std::fs::write(dir.join("accepting-daa.txt"), ctx.daa_score.to_string()).unwrap();
            std::fs::write(dir.join("PROVENANCE.txt"), "Generated by g14_native_funded_candidate_fixture_generator_v1 from existing tiny original fixture. Public test MLDSA seed: 0x6d repeated 32. Fresh unclaimed registered state; tiny execution regenerated through palw_tir_public_frame_fixture_ruled_v1 in the real key-bearing FP job context (key changes job_id); current producer key/operator/payout replaced; original FP and source pledge re-signed; exact production prepare_original_v1 succeeded; no funded escrow installed. Parent records exact source/bytes SHA256 externally.\n").unwrap();
        }
    }

