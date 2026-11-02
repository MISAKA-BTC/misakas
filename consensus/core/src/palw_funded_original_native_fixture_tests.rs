    /// Explicit artifact generator; never enables fixture constructors in production.
    #[test]
    fn g14_native_funded_candidate_fixture_generator_v1() { run_family_funded_native_fixture_v1(false,false,false); }
    #[test]
    fn family_supplier_four_role_prefix_native_original_and_financial_transitions() { run_family_funded_native_fixture_v1(true,true,false); }
    #[test]
    fn family_supplier_ordinary_native_original_and_financial_transitions(){run_family_funded_native_fixture_v1(false,true,false);}
    #[test]
    fn rfc23s2_four_role_signed_material_scoped_checkpoint_reorg_restore() {run_family_funded_native_fixture_v1(true,true,true);}
    #[test]
    fn family_supplier_constraint_native_original_material_and_adjudication() {
        run_family_funded_native_fixture_mode_v1(false, true, false, true);
    }
    /// Producer data only: success does not certify native admission or settlement.
    #[test]
    #[ignore = "requires explicit fresh output, current native network/bond/card and prompt inputs"]
    fn family_supplier_ordinary_producer_material_only_generator_v1() {
        run_family_funded_native_fixture_generation_mode_v1(false,true,false,false,false,true);
    }
    fn run_family_funded_native_fixture_v1(prefix:bool,production:bool,scoped:bool) {
        run_family_funded_native_fixture_mode_v1(prefix, production, scoped, false);
    }
    fn run_family_funded_native_fixture_mode_v1(prefix:bool,production:bool,scoped:bool,constraint:bool) {
        run_family_funded_native_fixture_archive_mode_v12(prefix,production,scoped,constraint,false);
    }
    fn run_family_funded_native_fixture_archive_mode_v12(prefix:bool,production:bool,scoped:bool,constraint:bool,observe_archive:bool) {
        run_family_funded_native_fixture_generation_mode_v1(prefix,production,scoped,constraint,observe_archive,false);
    }
    fn family_fixture_budget_policy_v1()->crate::palw_bond_budget_v1::PalwBondBudgetPolicyV1 {
        // SAME existing native39 private P4/V3 policy (mining_config_v3_budget),
        // not public activation or a new E/Q exemption. Both reserve one claim.
        let mut policy=crate::palw_values_p4_v1::palw_bond_budget_policy_provisional_v1();
        policy.slice_rights_by_rho=true;
        policy
    }
    fn family_fixture_native_params_v1(params:PalwStateParamsV2)->PalwStateParamsV2 {
        let probe=crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(crate::config::params::ForkActivation::new(0));
        let budget=family_fixture_budget_policy_v1();
        params.with_family_supplier_from(Some(0))
            .with_claim_hard_deadline_v1(Some(crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1{from_daa:0,policy:probe.policy}))
            .with_court_clock_v2_from_daa(Some(0)).with_claim_retirement_daa(3_000).unwrap()
            .with_settlement_once_from(Some(0)).with_material_income_from(Some(0))
            .with_bond_budget(Some(crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1{from_daa:0,policy:budget,allocation:None}))
    }
    struct ScopedPrefixSourceHistoryV1 {
        state: PalwChainStateV2,
        params: PalwStateParamsV2,
        point: PalwBlockContextV2,
        claim: Hash64,
        export: crate::palw_claim_local_v1::prefix_v1::PalwSignedPrefixExportV1,
        identity: crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportIdentityV1,
    }
    fn run_family_funded_native_fixture_generation_mode_v1(prefix:bool,production:bool,scoped:bool,constraint:bool,observe_archive:bool,producer_material_only:bool) {
        let _=run_family_funded_native_history_v1(prefix,production,scoped,constraint,observe_archive,producer_material_only,None,None);
    }
    #[test]
    fn original_archive_funded_normal_source_and_prefix_lease_v19(){
        let _=run_family_funded_native_history_profile_v19(true,true,true,false,false,false,None,None,true,false);
    }
    #[test]
    fn original_archive_funded_normal_mixed_source_and_prefix_v22(){let _=run_family_funded_native_history_profile_v19(true,true,true,false,false,false,None,None,true,true);}
    fn run_family_funded_native_history_v1(
        prefix:bool,production:bool,scoped:bool,constraint:bool,observe_archive:bool,producer_material_only:bool,
        source_export_request:Option<crate::palw_freeprompt_v3::PalwFpPrefixStateV1>,source_prompt:Option<Vec<u32>>,
    )->Option<ScopedPrefixSourceHistoryV1> {
        run_family_funded_native_history_profile_v19(prefix,production,scoped,constraint,observe_archive,producer_material_only,source_export_request,source_prompt,false,false)
    }
    fn run_family_funded_native_history_profile_v19(
        prefix:bool,production:bool,scoped:bool,constraint:bool,observe_archive:bool,producer_material_only:bool,
        source_export_request:Option<crate::palw_freeprompt_v3::PalwFpPrefixStateV1>,source_prompt:Option<Vec<u32>>,archive_finance:bool,archive_mixed:bool,
    )->Option<ScopedPrefixSourceHistoryV1> {
        run_family_funded_native_history_geometry_v3(prefix,production,scoped,constraint,observe_archive,producer_material_only,source_export_request,source_prompt,archive_finance,archive_mixed,false)
    }
    #[test]
    fn original_archive_legacy_oar2_signed_source_and_prefix_replay_v3(){
        let _=run_family_funded_native_history_geometry_v3(true,true,true,false,false,false,None,None,true,true,true);
    }
    fn run_family_funded_native_history_geometry_v3(
        prefix:bool,production:bool,scoped:bool,constraint:bool,observe_archive:bool,producer_material_only:bool,
        source_export_request:Option<crate::palw_freeprompt_v3::PalwFpPrefixStateV1>,source_prompt:Option<Vec<u32>>,archive_finance:bool,archive_mixed:bool,archive_legacy_geometry:bool,
    )->Option<ScopedPrefixSourceHistoryV1> {
        assert!(!archive_legacy_geometry||(archive_finance&&archive_mixed));
        if archive_finance{assert!(production&&scoped&&!constraint&&!observe_archive&&!producer_material_only);}
        if producer_material_only {
            assert!(production&&!prefix&&!scoped&&!constraint&&!observe_archive);
            let dir=std::path::PathBuf::from(std::env::var_os("MISAKA_FAMILY_NATIVE_FIXTURE_DIR").expect("producer data generation requires an explicit output directory"));
            assert!(dir.is_absolute()&&!dir.exists(),"producer data output must be fresh and absolute");
            for name in ["MISAKA_FAMILY_EXECUTOR_TEST_CARD","MISAKA_FAMILY_NETWORK_DOMAIN","MISAKA_FAMILY_EXECUTOR_BOND","MISAKA_FAMILY_PROMPT_IDS_V1"] {
                assert!(std::env::var_os(name).is_some(),"producer data generation requires explicit {name}");
            }
        }
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
        let source_fault=source_export_request.is_none()&&production&&!prefix&&match std::env::var("MISAKA_FAMILY_SOURCE_FAULT"){
            Ok(value)=>match value.as_str(){"1"=>true,"0"=>false,_=>panic!("explicit family source fault must be 0 or 1")},
            Err(std::env::VarError::NotPresent)=>false,Err(e)=>panic!("family source fault: {e}"),
        };
        if producer_material_only {assert!(!source_fault,"ordinary producer data mode requires an honest execution");}
        let prompt_override=if source_prompt.is_some(){source_prompt}else{match std::env::var("MISAKA_FAMILY_PROMPT_IDS_V1") {
            Ok(raw)=>{
                assert!(production&&!prefix&&!constraint,"explicit prompt is only for ordinary native producer captures");
                let fields=raw.split(',').take(7).collect::<Vec<_>>();
                assert_eq!(fields.len(),6,"the native capture fixture has exactly six prompt tokens");
                Some(fields.into_iter().map(|s|s.parse::<u32>().expect("explicit prompt contains only u32 token IDs")).collect::<Vec<_>>())
            }
            Err(std::env::VarError::NotPresent)=>None,Err(e)=>panic!("explicit producer prompt: {e}"),
        }};
        let (fx,family_semantics,family_eof,family_raw,family_table)=if constraint {
            assert!(production && !prefix);
            let (fx,sem,eof,raw,table)=crate::palw_tir_public_claim_v1::palw_tir_public_family_constraint_frame_with_executor_network_fixture_v1(&key,fixture_network);
            (fx,Some(sem),Some(eof),Some(raw),Some(table))
        } else if prefix {
            let (fx,sem,eof,raw,table)=crate::palw_tir_public_claim_v1::palw_tir_public_prefix_tokenizer_frame_with_executor_network_fixture_v1(&key,fixture_network);
            (fx,Some(sem),Some(eof),Some(raw),Some(table))
        } else if production {
            let (fx,sem,eof,raw,table)=if let Some(prompt)=prompt_override.as_deref(){
                crate::palw_tir_public_claim_v1::palw_tir_public_family_source_frame_with_executor_network_prompt_fixture_v1(&key,fixture_network,source_fault,prompt)
            }else{
                crate::palw_tir_public_claim_v1::palw_tir_public_family_source_frame_with_executor_network_fixture_v1(&key,fixture_network,source_fault)
            };
            (fx,Some(sem),Some(eof),Some(raw),Some(table))
        } else {
            (crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |job| job.executor_pubkey=key.clone(),None),None,None,None,None)
        };
        // Execute a distinct earlier source, preserving the actually consumed
        // prefix tokens and changing only its unconsumed prompt suffix.
        let source_history=if production&&scoped&&prefix {
            assert!(source_export_request.is_none());
            let consumed=match &fx.frame.original_v1().job.tail {
                Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Prefix(p))=>p.clone(),
                _=>panic!("actual scoped Prefix job"),
            };
            let mut prompt=fx.execution.prompt.clone();
            assert!(prompt.len()>consumed.prefix_tokens as usize);
            let last=prompt.len()-1;prompt[last]=(prompt[last]+1)%fx.record.logits_vocab;
            assert_ne!(prompt,fx.execution.prompt);
            assert_eq!(&prompt[..consumed.prefix_tokens as usize],&fx.execution.prompt[..consumed.prefix_tokens as usize]);
            Some(run_family_funded_native_history_geometry_v3(false,true,true,false,false,false,Some(consumed),Some(prompt),archive_finance,archive_mixed,archive_legacy_geometry).unwrap())
        }else{None};
        if let Some(prefix)=source_export_request.as_ref(){
            let space=crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&fx.execution.binding.class).unwrap();
            let digest=crate::palw_tir_prefix_state_v1::palw_tir_prefix_kv_digest_v1(&space,&fx.execution.binding.job_context,prefix.prefix_tokens,
                |leaf|Ok(fx.execution.preimages[leaf.index as usize].values_le.clone())).unwrap();
            assert_eq!(crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&prefix.class_id,prefix.prefix_tokens,&digest),prefix.state_root,
                "independently executed source supplies exact consumed K/V");
        }
        let (world, params, mut extras) = world(&fx, &fx.frame);
        let params=if production {
            let shipped=crate::config::params::palw_t12_shipped_params();
            let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)=shipped.palw_consensus_mode else {panic!("shipped T12 must supply the adopted FP price")};
            assert!(bundle.state.fp_quanta_per_canonical_job()>0&&bundle.state.fp_max_quanta_per_receipt()>0);
            params.with_fp_quanta(bundle.state.fp_quanta_per_canonical_job(),bundle.state.fp_max_quanta_per_receipt()).unwrap()
        }else{params};
        let params=if production {params.with_court_outsider_lane_from_daa(Some(0)).with_legacy_public_filer_from_daa(Some(0)).with_family_supplier_from(Some(0))}else{params};
        if production {
            // Same mirror as virtual_processor/processor/palw_accept.rs:
            // offence_attribution_active = palw_offence_attribution_at(daa).
            // The old direct-court world defaults it off; actual public FP
            // admission must retain its authenticated job pin in the new claim.
            let shipped=crate::config::params::palw_t12_shipped_params();
            extras.offence_attribution_active=shipped.palw_offence_attribution.is_some_and(|f|f.is_active(fx.frame.plan_v1().terms.accepted_daa));
            assert!(extras.offence_attribution_active,"production family fixture requires the actual active job-identity fence");
        }
        if production {extras.tir_public_conviction_active=true;extras.tir_public_network_domain_v1=Some(fixture_network);extras.staged_tir_route_active=true;extras.staged_public_court=Some(fx.rules.config);}
        let params=if scoped {
            let mut p=params.with_claim_local_settlement_from_daa(Some(0));
            if prefix||source_export_request.is_some(){
                // Existing private finite Prefix-export mechanics policy only.
                p.econ_contracts_v1.horizon_extension=Some((fx.frame.plan_v1().terms.accepted_daa,
                    crate::palw_econ_contracts_v1::PalwHorizonExtensionPolicyV1::unapproved_probe_v1()));
            }
            p
        }else{params};
        // Resolve the SAME existing private composition before any scoped rule
        // digest, quote or signature; included blocks only re-read these mirrors.
        let params=if production{family_fixture_native_params_v1(params)}else{params};
        // All private composition is fixed before native genesis/ruleset and
        // source signatures. The value is the same later prospective FP DAA.
        let params=if archive_finance{let mut p=params;p.econ_contracts_v1.horizon_extension=Some((fx.frame.plan_v1().terms.accepted_daa,crate::palw_econ_contracts_v1::PalwHorizonExtensionPolicyV1::unapproved_probe_v1()));p}else{params};
        if scoped {
            // Private fixture rule identity is derived BEFORE the original
            // admission. This is a material/rollback regression, not a public
            // activation/measurement or independently certified credit record.
            let wire=borsh::to_vec(&params).unwrap();
            let digest=blake2b_simd::Params::new().hash_length(32).key(b"misaka/test/scoped-state-policy/v2").hash(&wire);
            let mut policy=[0;32];policy.copy_from_slice(digest.as_bytes());
            let fired=blake2b_simd::Params::new().hash_length(32).key(b"misaka/test/scoped-fences/v2").hash(&[1]);
            let mut fences=[0;32];fences.copy_from_slice(fired.as_bytes());
            extras.claim_local_rule_context_v1=Some((policy,fences));
        }
        // Explicit tiny fixture policy, not a production preset change.
        let mut globals = crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1;
        globals.seat_count = 1;
        extras.model_registry = Some(crate::palw_model_registry_v1::PalwModelRegistryFoldV1 {
            globals, span_daa: 600, genesis_works: Default::default(), grace_until_daa: 0,
            admission_audit_period_daa: None, readiness_v2_active: false, bond_maturity: None,
        });
        let native_fixture_genesis=if archive_mixed{
            let mut configured=crate::config::params::palw_t12_shipped_params();
            let genesis_objects=match &mut configured.palw_consensus_mode{crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)=>{bundle.state=params.clone();bundle.genesis_objects.clone()},_=>panic!("native mixed fixture needs configured genesis objects")};
            let actual_domain=crate::palw_attempt_v2::palw_network_domain_v2_for(configured.net.to_string().as_bytes(),Some(configured.genesis.hash));
            assert_eq!(fixture_network,actual_domain,"native core bootstrap must use its configured genesis domain");
            extras.native_fact_ruleset=Some(configured.consensus_params_id());
            extras.kernel_route=Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1{network_domain:fixture_network,ruleset_digest:h(0x42),chain_genesis:configured.genesis.hash,..Default::default()});
            Some((configured,genesis_objects))
        }else{None};
        let mut state=if let Some(history)=source_history.as_ref(){history.state.clone()}else if let Some((configured,objects))=native_fixture_genesis.as_ref(){
            // Actual native bare-genesis fold BEFORE fixture registration/bond
            // setup. No first FP/empty midchain view is labeled genesis.
            let point=PalwBlockContextV2{block:configured.genesis.hash,daa_score:configured.genesis.daa_score,blue_score:0,subsidy:0};
            let (native,_genesis_delta)=apply_palw_transition_v2_with_extras(&PalwChainStateV2::genesis(),&params,&point,objects,None,false,false,false,false,&extras).unwrap();
            assert!(native.claims.is_empty());assert_eq!(native.last_point(),Some(&point));
            assert_eq!(super::super::palw_claim_local_native_cohort_baseline_v22::expected_native_root_births_v22(&native,fx.frame.original_v1().execution_root,point).unwrap(),Some(0));
            super::super::palw_claim_local_native_cohort_baseline_v22::validate_network_v22(&native,configured).unwrap();
            native
        }else{PalwChainStateV2::genesis()};
        if source_history.is_none(){state.bonds=world.bonds.clone();}
        let family_filer_key=mldsa::generate_key_pair([0x6e;32]);
        {let filer=state.bonds.get_mut(&outsider()).unwrap();let pubkey=family_filer_key.verification_key.as_ref().to_vec();
            if source_history.is_some(){assert_eq!(filer.pubkey,pubkey);assert_eq!(filer.operator_id,palw_operator_id_v2(&pubkey));assert_eq!(filer.payout_payload,kaspa_hashes::blake2b_512_address_payload(&pubkey));}
            else{filer.pubkey=pubkey;filer.operator_id=palw_operator_id_v2(&filer.pubkey);filer.payout_payload=kaspa_hashes::blake2b_512_address_payload(&filer.pubkey);}}
        if source_history.is_none(){
            state.classes=world.classes.clone();state.class_shares=world.class_shares.clone();state.tir_classes=world.tir_classes.clone();
        }else{assert_eq!(state.tir_classes,world.tir_classes,"reuse actually registered immutable source class");}
        let class = fx.frame.original_v1().job.class_id;
        if let Some(sem)=&family_semantics {if source_history.is_some(){assert_eq!(state.class_semantics.get(&class),Some(sem));}else{state.class_semantics.insert(class,sem.clone());}extras.class_semantics_active=true;}
        if production {
            let bytes=crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(family_table.as_ref().unwrap());
            let checked=crate::palw_court_family_material_v1::prepare_family_table_inventory_v1(&bytes,family_semantics.as_ref().unwrap(),fx.record.logits_vocab).unwrap();
            extras.family_supplier_active=true;
            extras.family_original_table_inventories_v2.insert(fx.frame.plan_v1().original_claim,checked);
        }
        if let Some(eof)=&family_eof {let row=Row::ArtifactInventory(eof.clone());if source_history.is_some(){assert_eq!(state.staged_courts.get(&Key::Inventory(eof.artifact_hash)),Some(&row));}else{state.staged_courts.insert(Key::Inventory(eof.artifact_hash),row);}}
        if source_history.is_some(){assert_eq!(state.staged_courts.get(&Key::TirLayout(class)),world.staged_courts.get(&Key::TirLayout(class)));}else{state.staged_courts.insert(Key::TirLayout(class),world.staged_courts[&Key::TirLayout(class)].clone());}
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
        if source_history.is_some(){assert_eq!(bond.pubkey,key);assert_eq!(bond.operator_id,palw_operator_id_v2(&key));assert_eq!(bond.payout_payload,kaspa_hashes::blake2b_512_address_payload(&key));}
        else{bond.pubkey=key.clone();bond.operator_id=palw_operator_id_v2(&key);bond.payout_payload=kaspa_hashes::blake2b_512_address_payload(&key);}
        if production {
            let capital=family_fixture_budget_policy_v1().capital_unit_sompi;
            if source_history.is_none(){
                // Preserve the existing operating principal and fund one actual
                // policy capital unit before quote/admission;
                // later consumers retain the real source's remaining balance.
                bond.collateral=bond.collateral.checked_add(capital).unwrap();
            }else{assert!(bond.collateral>=capital,"retain the prior native source's actually funded capital");}
        }
        let mut object = fx.object.clone();
        if let PalwConsensusObjectV2::FreePromptCommitted {
            claim:c,class_id,bond,executor_pubkey,work_leaves,prompt_token_ids_hash,prompt_tokens,prompt_token_ids,
            decode_tokens_executed,trace_root,output_root,execution_root,trace_chunk_count,trace_retention_daa,
            public_job,job_pin,consumed_prefix_state,eval,
        } = &mut object {
            let original=&fp.commitment;
            *c=claim;*class_id=original.job.class_id;*bond=producer;*executor_pubkey=original.job.executor_pubkey.clone();
            *work_leaves=original.work_leaves;*prompt_token_ids_hash=original.job.prompt_token_ids_hash;
            *prompt_tokens=original.job.prompt_tokens;*prompt_token_ids=fp.prompt_token_ids.clone();
            *decode_tokens_executed=original.decode_tokens_executed;
            *trace_root=original.trace_root;*output_root=original.output_root;*execution_root=original.execution_root;
            *trace_chunk_count=original.trace_chunk_count;*trace_retention_daa=original.trace_retention_daa;
            *public_job=Some(Box::new(original.clone()));*job_pin=crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original);
            *consumed_prefix_state=fp.consumed_prefix_state_v1();*eval=None;
        } else { panic!("original fixture object"); }
        let ctx=PalwBlockContextV2{block:if source_export_request.is_some(){h(0xF134)}else{h(0xF135)},
            daa_score:fx.frame.plan_v1().terms.accepted_daa,
            blue_score:source_history.as_ref().map_or(1,|h|h.point.blue_score.checked_add(1).unwrap()),subsidy:0};
        if let Some(history)=source_history.as_ref(){
            assert_ne!(history.claim,claim);assert_ne!(history.point.block,ctx.block);
            assert_eq!(state.last_point(),Some(&history.point));
            assert!(history.point.daa_score<=ctx.daa_score&&history.point.blue_score<ctx.blue_score);
        }
        let mut params=params;

        if observe_archive {
            assert!(production && scoped && !prefix && !constraint);
            params.econ_contracts_v1.horizon_extension=Some((ctx.daa_score,crate::palw_econ_contracts_v1::PalwHorizonExtensionPolicyV1::unapproved_probe_v1()));
            extras.original_archive_data_observer_v16=true;
        }
        let builder = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
        let (plan, _, _, _) = original_plan_v1(&builder, ctx.daa_score, &fp.commitment, fx.frame.header_v1()).unwrap();
        drop(builder);
        let mut pledge = fx.frame.custody_v1().pledge_v1().clone();
        pledge.claim = claim;
        pledge.job_identity = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
        pledge.resource_plan_id = plan.resource_plan_id;
        pledge.source_id = plan.source_id;
        pledge.material_until_daa=plan.terms.material_until_daa;
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
        if production&&params.court_clock_v2_from_daa().is_some_and(|from|ctx.daa_score>=from){
            let expected=crate::palw_court_clock_v2::palw_court_clock_v2_required_retention_with(ctx.daa_score,
                palw_da_disclose_window_daa_v1(&params),crate::palw_court_clock_v2::palw_court_clock_v2_reorg_retention(extras.staged_public_court.as_ref())).unwrap();
            assert_eq!(plan.acquisition.clock.version,crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4);
            assert!(plan.terms.material_until_daa>=expected);
            assert_eq!(pledge.material_until_daa,plan.terms.material_until_daa);
            let mut short=pledge.clone();short.material_until_daa=plan.terms.material_until_daa.checked_sub(1).unwrap();
            short.signature=mldsa::sign(&kp.signing_key,short.id_v1().as_byte_slice(),
                crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1,seed).unwrap().as_ref().to_vec();
            let before=builder.state.state_root();
            let why=prepare_original_v1(&builder,&ctx,&object,&fp.commitment,&envelope.header,&short).unwrap_err();
            assert!(matches!(why,PalwStateV2Error::CarriageInconsistent(ref s)if s=="TIR custody changes its original Panel, terms, literal scope or credited grants"),"valid signed short material remains refused: {why:?}");
            assert_eq!(builder.state.state_root(),before);
        }
        if let Some(prefix)=source_export_request.as_ref(){
            use crate::palw_claim_local_v1::prefix_v1::*;
            let intent=PalwPrefixExportIntentV1{version:1,network:fp.commitment.job.network_domain,source_namespace:1,
                source_nonce:h(0xF13440),base_commitment:claim,producer_key_id:crate::mldsa87_primitives::mldsa87_key_id(&key),ordinal:0,
                prefix:prefix.clone(),liability_until:fp.commitment.trace_retention_daa,material_until:fp.commitment.trace_retention_daa};
            let signature=mldsa::sign(&kp.signing_key,intent.id_v1().as_byte_slice(),PREFIX_EXPORT_CONTEXT_V1,seed).unwrap().as_ref().to_vec();
            extras.prefix_dependencies_v1.insert(claim,PalwPrefixDependencyEnvelopeV1{inner:envelope.encode_v1().unwrap(),
                exports:vec![PalwSignedPrefixExportV1{intent,signature}],consumed:None});
        }
        if let Some(history)=source_history.as_ref(){
            use crate::palw_claim_local_v1::prefix_v1::*;
            let consumed=fp.consumed_prefix_state_v1();assert!(!consumed.is_genesis());
            let intent=PalwPrefixUseIntentV1{version:1,network:fp.commitment.job.network_domain,consumer_nonce:h(0xF13540),
                base_commitment:claim,consumer_key_id:crate::mldsa87_primitives::mldsa87_key_id(&key),source:history.identity,prefix:consumed,
                hard_deadline:ctx.daa_score.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).unwrap(),material_until:fp.commitment.trace_retention_daa};
            intent.check_source_v1(&history.export.intent,&history.identity,fixture_network,claim,ctx.daa_score,ctx.blue_score).unwrap();
            for mode in 0..3 {
                let mut foreign=intent.clone();
                match mode {0=>foreign.network=h(0xF14001),1=>foreign.base_commitment=h(0xF14002),_=>foreign.source.intent=h(0xF14003)}
                assert!(foreign.check_source_v1(&history.export.intent,&history.identity,fixture_network,claim,ctx.daa_score,ctx.blue_score).is_err(),
                    "actual source identity never accepts another network, base claim or export");
            }
            let signature=mldsa::sign(&kp.signing_key,intent.id_v1().as_byte_slice(),PREFIX_USE_CONTEXT_V1,seed).unwrap().as_ref().to_vec();
            extras.prefix_dependencies_v1.insert(claim,PalwPrefixDependencyEnvelopeV1{inner:envelope.encode_v1().unwrap(),exports:vec![],
                consumed:Some(PalwSignedPrefixUseV1{intent,signature})});
        }
        if observe_archive {
            use crate::palw_claim_local_v1::prefix_v1::*;
            // A real signed export declaration, not a claim-computation verdict.
            // Acceptance and quote bind its exact bytes; later Court may refute
            // a declared prefix root independently of data/fee accounting.
            let intent=PalwPrefixExportIntentV1 {
                version:1,network:fp.commitment.job.network_domain,source_namespace:1,
                source_nonce:h(0xA2C120),base_commitment:claim,
                producer_key_id:crate::mldsa87_primitives::mldsa87_key_id(&key),ordinal:0,
                prefix:crate::palw_freeprompt_v3::PalwFpPrefixStateV1 {
                    class_id:class,prefix_tokens:2,state_root:fp.commitment.execution_root,
                },
                liability_until:fp.commitment.trace_retention_daa-1,
                material_until:fp.commitment.trace_retention_daa,
            };
            let signature=mldsa::sign(&kp.signing_key,intent.id_v1().as_byte_slice(),PREFIX_EXPORT_CONTEXT_V1,seed).unwrap().as_ref().to_vec();
            extras.prefix_dependencies_v1.insert(claim,PalwPrefixDependencyEnvelopeV1 {
                inner:envelope.encode_v1().unwrap(),exports:vec![PalwSignedPrefixExportV1{intent,signature}],consumed:None,
            });
        }
        // Retain only requested public legacy transport bytes before native
        // construction takes ownership of the original envelope. The dump is
        // the unclaimed prestate, rather than the transformed funding.
        let legacy_fixture_literal=if source_history.is_none()&&std::env::var_os("MISAKA_G14_NATIVE_FIXTURE_DIR").is_some(){
            Some(envelope.encode_v1().unwrap())
        }else{None};
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
        let restore_params=source_history.as_ref().map_or(&params,|h|&h.params);
        let loaded = carriage.clone().into_state(restore_params, Some(state.state_root())).unwrap();
        assert_eq!(loaded.state_root(), state.state_root());
        if source_history.is_none(){if let Ok(dir) = std::env::var("MISAKA_G14_NATIVE_FIXTURE_DIR") {
            let dir = std::path::Path::new(&dir);
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("state.borsh"), borsh::to_vec(&carriage).unwrap()).unwrap();
            std::fs::write(dir.join("params.borsh"), borsh::to_vec(&params).unwrap()).unwrap();
            std::fs::write(dir.join("original.tfp1"), legacy_fixture_literal.as_ref().expect("requested legacy prestate literal retained before native construction")).unwrap();
            std::fs::write(dir.join("accepting-daa.txt"), ctx.daa_score.to_string()).unwrap();
            std::fs::write(dir.join("PROVENANCE.txt"), "Generated by g14_native_funded_candidate_fixture_generator_v1 from existing tiny original fixture. Public test MLDSA seed: 0x6d repeated 32. Fresh unclaimed registered state; tiny execution regenerated through palw_tir_public_frame_fixture_ruled_v1 in the real key-bearing FP job context (key changes job_id); current producer key/operator/payout replaced; original FP and source pledge re-signed; exact production prepare_original_v1 succeeded; no funded escrow installed. Parent records exact source/bytes SHA256 externally.\n").unwrap();
        }}
        None
    }

#[test]
fn rfc23s2_pre_funding_native_archive_adjunct_quote_binding_and_bounds(){run_family_funded_native_fixture_v1(false,false,false); }

#[test]
fn rfc23s2_born_source_archive_quote_uses_actual_install_spans_and_cold_undo(){
    run_family_funded_native_fixture_archive_mode_v12(false,true,true,false,true);
}
