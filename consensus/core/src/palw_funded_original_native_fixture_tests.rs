    /// Explicit artifact generator; never enables fixture constructors in production.
    #[test]
    fn g14_native_funded_candidate_fixture_generator_v1() { run_family_funded_native_fixture_v1(false,false); }
    #[test]
    fn family_supplier_four_role_prefix_native_original_and_financial_transitions() { run_family_funded_native_fixture_v1(true,true); }
    #[test]
    fn family_supplier_ordinary_native_original_and_financial_transitions(){run_family_funded_native_fixture_v1(false,true);}
    fn run_family_funded_native_fixture_v1(prefix:bool,production:bool) {
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;
        use crate::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PALW_FP_V3_VERSION, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT};
        // Optional native-node harness card, selected before job construction.
        // This is the same public TEST seed convention as TestConsensus; no
        // operator secret or already captured execution is rewritten.
        let seed = if production {match std::env::var("MISAKA_FAMILY_EXECUTOR_TEST_CARD") {
            Ok(value)=>{let card=value.parse::<u8>().expect("executor TEST card must be 0..15");assert!(card<16,"executor TEST card must be 0..15");let mut seed=[0xB0;32];seed[0]=0xB0+card;seed},
            Err(std::env::VarError::NotPresent)=>[0x6D;32],Err(e)=>panic!("executor TEST card environment: {e}"),
        }}else{[0x6D;32]};
        let kp = mldsa::generate_key_pair(seed);
        let key = kp.verification_key.as_ref().to_vec();
        // The key participates in fp_job_id. Re-execute the existing tiny
        // fixture in the changed job's OWN context; never transplant old roots.
        let fixture_network=if production {
            match std::env::var("MISAKA_FAMILY_NETWORK_DOMAIN") {
                Ok(value)=>value.parse::<Hash64>().expect("explicit family fixture network domain must be a complete Hash64"),
                Err(std::env::VarError::NotPresent)=>{
                    let actual=crate::config::params::palw_t12_shipped_params();
                    crate::palw_attempt_v2::palw_network_domain_v2_for(actual.net.to_string().as_bytes(),Some(actual.genesis.hash))
                }
                Err(e)=>panic!("family fixture network domain: {e}"),
            }
        }else{h(0x4E7)};
        let source_fault=production&&!prefix&&match std::env::var("MISAKA_FAMILY_SOURCE_FAULT"){
            Ok(value)=>match value.as_str(){"1"=>true,"0"=>false,_=>panic!("explicit family source fault must be 0 or 1")},
            Err(std::env::VarError::NotPresent)=>false,Err(e)=>panic!("family source fault: {e}"),
        };
        let (fx,family_semantics,family_eof,family_raw,family_table)=if prefix {
            let (fx,sem,eof,raw,table)=crate::palw_tir_public_claim_v1::palw_tir_public_prefix_tokenizer_frame_with_executor_network_fixture_v1(&key,fixture_network);
            (fx,Some(sem),Some(eof),Some(raw),Some(table))
        } else if production {
            let (fx,sem,eof,raw,table)=crate::palw_tir_public_claim_v1::palw_tir_public_family_source_frame_with_executor_network_fixture_v1(&key,fixture_network,source_fault);
            (fx,Some(sem),Some(eof),Some(raw),Some(table))
        } else {
            (crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |job| job.executor_pubkey=key.clone(),None),None,None,None,None)
        };
        let (world, params, mut extras) = world(&fx, &fx.frame);
        let params=if production {params.with_court_outsider_lane_from_daa(Some(0)).with_legacy_public_filer_from_daa(Some(0)).with_family_supplier_from(Some(0))}else{params};
        if production {extras.tir_public_conviction_active=true;extras.tir_public_network_domain_v1=Some(fixture_network);extras.staged_tir_route_active=true;extras.staged_public_court=Some(fx.rules.config);}
        // Explicit tiny fixture policy, not a production preset change.
        let mut globals = crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;
        globals.seat_count = 1;
        extras.model_registry = Some(crate::palw_model_registry_v1::PalwModelRegistryFoldV1 {
            globals, span_daa: 600, genesis_works: Default::default(), grace_until_daa: 0,
            admission_audit_period_daa: None, readiness_v2_active: false, bond_maturity: None,
        });
        let mut state = PalwChainStateV2::genesis();
        state.bonds = world.bonds.clone();
        let family_filer_key=mldsa::generate_key_pair([0x6e;32]);
        {let filer=state.bonds.get_mut(&outsider()).unwrap();filer.pubkey=family_filer_key.verification_key.as_ref().to_vec();
            filer.operator_id=palw_operator_id_v2(&filer.pubkey);filer.payout_payload=kaspa_hashes::blake2b_512_address_payload(&filer.pubkey);}
        state.classes = world.classes.clone();
        state.class_shares = world.class_shares.clone();
        state.tir_classes = world.tir_classes.clone();
        let class = fx.frame.original_v1().job.class_id;
        if let Some(sem)=&family_semantics {state.class_semantics.insert(class,sem.clone());extras.class_semantics_active=true;}
        if production {
            let bytes=crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(family_table.as_ref().unwrap());
            let checked=crate::palw_court_family_material_v1::prepare_family_table_inventory_v1(&bytes,family_semantics.as_ref().unwrap(),fx.record.logits_vocab).unwrap();
            extras.family_supplier_active=true;
            extras.family_original_table_inventories_v2.insert(fx.frame.plan_v1().original_claim,checked);
        }
        if let Some(eof)=&family_eof {state.staged_courts.insert(Key::Inventory(eof.artifact_hash),Row::ArtifactInventory(eof.clone()));}
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
        if let PalwConsensusObjectV2::FreePromptCommitted { claim:c, public_job, executor_pubkey, job_pin, consumed_prefix_state, .. } = &mut object {
            *c = claim;
            *public_job = Some(Box::new(fp.commitment.clone()));
            *executor_pubkey = key.clone();
            *job_pin = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
            *consumed_prefix_state=fp.consumed_prefix_state_v1();
        } else { panic!("original fixture object"); }
        let ctx = PalwBlockContextV2 { block: h(0xF135), daa_score: fx.frame.plan_v1().terms.accepted_daa, blue_score: 1, subsidy: 0 };
        let builder = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
        let (plan, _, _, _) = original_plan_v1(&builder, ctx.daa_score, &fp.commitment, fx.frame.header_v1()).unwrap();
        drop(builder);
        let mut pledge = fx.frame.custody_v1().pledge_v1().clone();
        pledge.claim = claim;
        pledge.job_identity = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
        pledge.resource_plan_id = plan.resource_plan_id;
        pledge.source_id = plan.source_id;
        pledge.signature = mldsa::sign(&kp.signing_key, pledge.id_v1().as_byte_slice(),
            crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1, seed).unwrap().as_ref().to_vec();
        fp.validate_signature_v3(crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1).unwrap();
        let mut envelope = crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1::new_v1(
            borsh::to_vec(&fp).unwrap(), fx.frame.header_v1().clone(), pledge.clone(), plan.terms.plan.rent_sompi).unwrap();
        if let Some(semantic)=plan.semantic.as_ref(){
            let mut history=crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1{
                version:1,claim,producer,execution_root:fp.commitment.execution_root,
                job_identity:crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment),
                cu_ruleset_id:family_semantics.as_ref().unwrap().cu_ruleset_id,
                resource_plan_id:plan.semantic_resource_plan_id.unwrap(),maximum_records:plan.maximum_semantic_history_records,
                maximum_custodians:plan.acquisition.maximum_custodians,accept_until_daa:ctx.daa_score+1,
                material_until_daa:semantic.terms.material_until_daa.max(plan.terms.material_until_daa)
                    .max(ctx.daa_score+crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA+fx.rules.config.reorg_retention_daa),signature:Vec::new()};
            history.signature=mldsa::sign(&kp.signing_key,history.pledge_id_v1().as_byte_slice(),crate::palw_history_custody_v1::PALW_HISTORY_PLEDGE_CONTEXT_V1,seed).unwrap().as_ref().to_vec();
            envelope=envelope.with_history_pledge_v1(history.clone()).unwrap();extras.history_custody_pledges.insert(claim,history);
        }
        let builder=TransitionBuilder::try_new(&state,&params,false,false,false,false,&extras).unwrap();
        // Reuse the real original preparation, including producer pledge, source,
        // registered geometry and semantic bundle. This does not install a claim.
        prepare_original_v1(&builder, &ctx, &object, &fp.commitment, &envelope.header, &pledge).unwrap();
        if !production {
            include!("palw_tir_prepared_original_geometry_native_test_block.rs");
            include!("palw_tir_prepared_original_requirements_native_test_block.rs");
        }
        if production {
            include!("palw_court_family_supplier_production_original_test_block.rs");
        } else {
            include!("palw_funded_original_coordinator_native_test_block.rs");
        }
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
