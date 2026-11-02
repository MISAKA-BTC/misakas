// Included in the real native funded-original fixture. Native preparation and
// signatures below are real; the monetary subtransition fixture is deliberately
// not presented as an accepted block or multi-node campaign.
{
    use crate::palw_court_family_supplier_v1::*;
    use super::super::palw_court_family_supplier_fold_v1 as fs;
    let mut family_state=served.clone();
    let network=extras.tir_public_network_domain_v1.unwrap();
    let ruleset=h(0x42);
    if source_history.is_none()&&!archive_mixed{
        family_state.kernel_route=Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(network,ruleset),None,
            misaka_palw_kernel::rows::LedgerScalarsV1{daa:ctx.daa_score,burned:0}));
    }else{assert!(family_state.kernel_route.is_some(),"retain real prior source route and archives");}
    let budget_policy=family_fixture_budget_policy_v1();
    if source_history.is_none(){
        let mut book=crate::palw_bond_budget_v1::PalwBondBudgetStateV1::new(ctx.daa_score,&budget_policy);
        book.activate_material_income_v3(&mut Vec::new());family_state.bond_budget=Some(book);
    }else{assert!(family_state.bond_budget.is_some(),"retain actual source collateral and E reservation");}
    let family_params=params.clone().with_settlement_once_from(Some(0)).with_family_supplier_from(Some(0))
        .with_material_income_from(Some(0)).with_bond_budget(Some(crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1{
            from_daa:0,policy:budget_policy,allocation:None}));
    let mut family_extras=ok.clone();family_extras.family_supplier_active=true;family_extras.staged_tir_route_active=true;
    // Every real signed export/consume requires its actual admitted route header.
    let declares_route=family_extras.prefix_dependencies_v1.get(&claim).is_some_and(|e|!e.exports.is_empty()||e.consumed.is_some());
    if declares_route{
        family_extras.kernel_route=Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1{network_domain:network,ruleset_digest:ruleset,chain_genesis:if archive_mixed{native_fixture_genesis.as_ref().unwrap().0.genesis.hash}else{Hash64::default()},..Default::default()});
    }
    if let Some(history)=source_history.as_ref(){
        assert_eq!(borsh::to_vec(&family_params).unwrap(),borsh::to_vec(&history.params).unwrap(),"source/consumer share exact native policy");
    }
    let native=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&family_extras).unwrap();
    let req=prepare_paired_original_v1(&native,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap()
        .enrich_native_geometry_v1(&native,&ctx).unwrap().prepare_requirements_v1(&native,&ctx,
            PreparedRequirementBudgetsV1{numerical:budgets.0,input:budgets.1,source:budgets.2}).unwrap();
    let program=crate::palw_native_resource_v1::decode_native_tir_program_v1(family_state.tir_class_v1(&class).unwrap().program.as_slice()).unwrap();
    let graph=req.source_graph_v1().unwrap();
    let service_profile_id=family_state.court_service_state_v1().map(|b|b.profile_id).unwrap_or_else(||crate::palw_court_service_state_v2::court_service_profile_id_v2(crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.id()));
    let profile=crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(service_profile_id);
    let mut roles=vec![FamilySupplierRoleV1::ParameterAndPrompt];
    if family_semantics.as_ref().is_some_and(|sem|sem.token_table_root.is_some()){roles.push(FamilySupplierRoleV1::SemanticTable);}
    if family_semantics.as_ref().is_some_and(|sem|sem.raw_tokenizer_bytes.is_some()){roles.push(FamilySupplierRoleV1::TokenizerArtifact);}
    if prefix {roles.push(FamilySupplierRoleV1::PrefixGeneratedSelectionAndStop);}
    let table_bytes=family_table.as_ref().map(crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1);
    let checked_table=table_bytes.as_ref().map(|bytes|crate::palw_court_family_material_v1::prepare_family_table_inventory_v1(bytes,family_semantics.as_ref().unwrap(),req.geometry.source_v1().vocab_v1()).unwrap());
    let inventories=public_quote.inventories_v2();
    let old_quote=palw_family_original_native_quote_v2(&served,&params,&ctx,&ok,&object,table_bytes.as_deref()).unwrap();
    assert_eq!(old_quote.inventories_v2().len(),public_quote.inventories_v2().len());
    assert!(old_quote.inventories_v2().iter().all(|i|i.carrier_projection_v3().is_none()),"old API2 requires no new carrier birth sidecar");

    assert_eq!(inventories.iter().map(|i|i.role_v1()).collect::<Vec<_>>(),roles);
    for i in inventories {let facts=i.carrier_projection_v3().unwrap();assert_eq!(facts.input_count,pairing.carrier().inputs.len() as u64);assert_eq!(facts.distinct_txids,1);assert_eq!(facts.input_bound,1+u64::from(facts.prefix_use.is_some()));}
    if prefix {
        use crate::palw_tir_public_source_v1::PalwTirPublicRequestV1 as Query;
        use crate::palw_court_shared_material_intake_v1::MaterialTargetV1 as Target;
        let semantic=inventories.iter().find(|i|i.role_v1()==FamilySupplierRoleV1::PrefixGeneratedSelectionAndStop).unwrap();
        let legacy=crate::palw_court_family_material_v1::derive_family_inventory_with_table_v1(&family_state,req.frame_v1(),req.geometry.source_v1(),graph,profile,&program,req.inputs_v1(),family_semantics.as_ref(),FamilySupplierRoleV1::PrefixGeneratedSelectionAndStop,checked_table.as_ref()).unwrap();
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(req.frame_v1(),req.geometry.source_v1(),profile).unwrap();
        assert_eq!(legacy.id_v1(),old_quote.inventories_v2().iter().find(|i|i.role_v1()==FamilySupplierRoleV1::PrefixGeneratedSelectionAndStop).unwrap().id_v1(),"historical native API2 still derives the exact complete role4 inventory");
        assert_ne!(semantic.id_v1(),legacy.id_v1(),"new semantic purpose cannot alias historical all-query role4 inventory");
        assert_eq!(graph.query_count_v3(),set.len(),"every original numerical/source branch remains reserved");
        let mut semantic_queries=0usize;let mut numerical_queries=0usize;
        for ordinal in 0..set.len() {
            let query=set.request_at(ordinal).unwrap();
            let target=crate::palw_court_family_material_v1::FamilyMaterialTargetV1::Input(Target::Unit{ordinal});
            match query {
                Query::RowsNode{..}|Query::TileNode{..}|Query::TraceTile{..}=>{semantic_queries+=1;assert!(semantic.target_bound_v1(target).is_ok());assert!(legacy.target_bound_v1(target).is_ok());}
                Query::StepNode{..}|Query::StepLeaf{..}|Query::StepRun{..}=>{numerical_queries+=1;assert!(semantic.target_bound_v1(target).is_err());assert!(legacy.target_bound_v1(target).is_ok());
                    for role in 0..graph.role_slots_v3(){assert!(graph.finish_node_v3(crate::palw_court_source_material_graph_v3::SourceMaterialTargetV3::Unit(ordinal),role).unwrap().is_some(),"native original retains every numeric all-role Finish");}}
                Query::Ids{..}=>{},
            }
        }
        assert!(semantic_queries>0&&numerical_queries>0);
        assert_eq!(semantic.targets_v1().len(),semantic_queries+1);
        assert!(semantic.target_bound_v1(crate::palw_court_family_material_v1::FamilyMaterialTargetV1::Input(Target::FullIds)).is_ok());
        assert!(semantic.targets_v1().len()<legacy.targets_v1().len());
        eprintln!("native generated semantic profile3 targets={} numeric-original-queries={} full-source-queries={} legacy-role4-targets={}",semantic.targets_v1().len(),numerical_queries,set.len(),legacy.targets_v1().len());
    }
    let signed_material=crate::palw_court_service_state_v2::tir_service_required_material_until_v1(req.frame_v1(),hard).unwrap();
    let consents=inventories.iter().enumerate().map(|(index,inventory)|{
        let offered=FamilySupplierTermsV1{version:1,network,ruleset,claim,source:inventory.source_v1(),relative_graph:inventory.relative_graph_v1(),
            role:inventory.role_v1(),inventory:inventory.id_v1(),supplier:producer,sponsor:producer,
            max_material_bytes:inventory.max_material_bytes_v1(),accept_from_daa:ctx.daa_score,accept_until_daa:ctx.daa_score,
            respond_until_daa:hard,material_until_daa:signed_material,fee_profile:h(1),capital_profile:h(1),retention_profile:h(1),settlement_policy:h(1),
            fee_sompi:1,collateral_sompi:1,nonce:h(0x4201+index as u64)};
        let terms=fs::palw_family_supplier_terms_quote_v3(inventory,offered).unwrap();
        sign_family_supplier_v1(terms,|_,message,context|Some(mldsa::sign(&kp.signing_key,message,context,seed).unwrap().as_ref().to_vec())).unwrap()
    }).collect::<Vec<_>>();
    let inventory=&inventories[0];let consent=consents[0].clone();let terms=consent.terms.clone();
    let total_fees=consents.iter().map(|c|c.terms.fee_sompi).sum::<u64>();
    let total_capital=consents.iter().map(|c|u128::from(c.terms.collateral_sompi)).sum::<u128>();
    let contract=consent.id_v1().unwrap();
    let make_plan=||fs::derive_family_supplier_plan_v1(&family_state,&ctx,&inventory,req.frame_v1().plan_v1().allocation_id,signed_material,&consent).unwrap();
    assert_eq!(make_plan().row.consent.terms,terms);
    let mut changed=ctx;changed.daa_score+=1;
    assert!(fs::derive_family_supplier_plan_v1(&family_state,&changed,&inventory,req.frame_v1().plan_v1().allocation_id,signed_material,&consent).is_err(),
        "an explicit singleton acceptance does not sign a later unknown birth");
    let wrapper=crate::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2{
        version:3,funded_original:pairing.envelope().encode_v1().unwrap(),suppliers:consents.clone(),semantic_table:family_table.as_ref().map(crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1)};
    let wire=wrapper.encode_v2().unwrap();
    let decoded=crate::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2::decode_v2(&wire).unwrap();
    assert_eq!(decoded,wrapper);
    let mut tx=pairing.carrier().clone();
    // End the first quote borrow before finalizing actual FPR2 bytes.
    drop(native);
    tx.payload=if let Some(provenance)=family_extras.prefix_dependencies_v1.get_mut(&claim){
        provenance.inner=wire.clone();provenance.encode_v1().unwrap()
    }else{wire.clone()};
    tx.finalize();
    let native=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&family_extras).unwrap().with_native_original_parent_v19(&family_state);
    let actual=palw_funded_original_pairing_payload_v1(&tx,&wire).unwrap();
    assert_eq!(actual.suppliers_v2(),consents.as_slice());assert_ne!(actual.outpoint(),pairing.outpoint());
    if production&&source_export_request.is_none()&&!archive_finance {if let Ok(dir)=std::env::var("MISAKA_FAMILY_NATIVE_FIXTURE_DIR"){
        let dir=std::path::Path::new(&dir);
        if producer_material_only {
            // Reserve the output atomically at the write boundary. Concurrent
            // creation must fail instead of overwriting somebody else's data.
            std::fs::create_dir_all(dir.parent().expect("absolute producer output has a parent")).unwrap();
            std::fs::create_dir(dir).expect("producer output must remain fresh at the write boundary");
        } else {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(dir.join("state.borsh"),borsh::to_vec(&PalwStateCarriageV2::from_state(&family_state)).unwrap()).unwrap();
        std::fs::write(dir.join("params.borsh"),borsh::to_vec(&family_params).unwrap()).unwrap();
        std::fs::write(dir.join("original.tfo2"),wrapper.encode_v2().unwrap()).unwrap();
        std::fs::write(dir.join("semantic-table.bin"),wrapper.semantic_table.as_ref().unwrap()).unwrap();
        std::fs::write(dir.join("raw-tokenizer.json"),family_raw.as_ref().unwrap()).unwrap();
        std::fs::write(dir.join("original.object.borsh"),borsh::to_vec(&object).unwrap()).unwrap();
        std::fs::write(dir.join("point.borsh"),borsh::to_vec(&ctx).unwrap()).unwrap();
        std::fs::write(dir.join("network-domain.txt"),network.to_string()).unwrap();
        std::fs::write(dir.join("executor-bond.txt"),format!("{}:{}",producer.0.transaction_id,producer.0.index)).unwrap();
        std::fs::write(dir.join("executor-seed.borsh"),borsh::to_vec(&seed).unwrap()).unwrap();
        std::fs::write(dir.join("fixture-kind.txt"),if prefix{"prefix-consumer"}else if source_fault{"faulty-ordinary-source"}else{"ordinary-source"}).unwrap();
        std::fs::write(dir.join("producer-execution.borsh"),borsh::to_vec(&(&fx.execution.binding,&fx.execution.preimages,&fx.execution.hashes,&fx.execution.ops,&fx.execution.prompt,&fx.execution.rows,&fx.execution.generated)).unwrap()).unwrap();
        std::fs::write(dir.join("PROVENANCE.txt"),if source_history.is_some(){"Scoped Prefix consumer fixture: prestate contains a distinct ordinary source actually admitted by the native signed TFO2 fold, its exact SourceOriginV8 export identity, source archive, collateral, supplier rows and service reservations. Source delta cold restore/undo/replay checked before consumer construction; no source Claim or accepted point is injected. Explicit private test policy mirrors only; not public activation, node UTXO validation or certified credit. Public TEST seeds remain fixture data.\n"}else{"Genuine reexecuted native family6prompt/1decode/vocab2 fixture; fixture-kind.txt identifies source/consumer. Prestate is explicitly seeded, not a network bootstrap. Public TEST executor/supplier seed in executor-seed.borsh (default0x6d or selectednativecard); funding payer0x7a, outsider0x6e. Pinned original/family schedules, native Source/graph/table/EOF inventories, c=1/DAA, no prior CourtService row. Node integration must regenerate/resign native policy scopes using its own accepted point/network before signing real UTXO inputs.\n"}).unwrap();
    }}

    if producer_material_only {
        // Everything above is signed producer material and read-only preparation.
        // No original transition, Claim installation or settlement ran in this mode.
        let dir=std::path::PathBuf::from(std::env::var_os("MISAKA_FAMILY_NATIVE_FIXTURE_DIR").expect("mandatory producer data output"));
        std::fs::write(dir.join("PROVENANCE.txt"),"Producer-material-only generation: genuine independently reexecuted ordinary six-token/vocab2 FP and real signed TFO2 wrapper. Explicit public TEST executor card, network domain, bond and prompt were selected before execution and signing. state.borsh is seeded declaration transport only; it is not accepted chain state or admission authority. No Claim was installed, no native block was accepted, and no financial/admission PASS is asserted. Normal node admission must rederive policy scopes and obtain real signatures/funding against its own current state. Existing full financial tests retain all assertions.\n").unwrap();
        assert!(family_state.claim(&claim).is_none(),"producer data must contain no accepted claim");
        eprintln!("[producer-material-only] generated {}; native admission and settlement untested",dir.display());
        return None;
    }

    let mut active=family_extras.clone();active.tir_funded_originals.insert(claim,actual.clone());
    let (active,actual,tx,wire,wrapper,envelope,archive_finance_quote)=include!("palw_original_archive_finance_fixture_setup_v1.rs");
    let mut b=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&active).unwrap().with_native_original_parent_v19(&family_state);
    if archive_finance{b.claim_local_input_basis_v12=Some(NativeFoldInputBasisV12::from_native_fold_v12(&ctx,std::slice::from_ref(&object),PalwBlockWorkV3::None,&[],Hash64::default()).unwrap());}
    // The family fence applies to native originals, not an unrelated ordinary
    // FP object with neither paired preparation nor a funding envelope.
    let mut ordinary_extras=active.clone();ordinary_extras.tir_funded_originals.remove(&claim);
    let ordinary=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&ordinary_extras).unwrap();
    assert!(admit_original_v1(&ordinary,&ctx,claim,None).unwrap().is_none());
    assert!(admit_original_v1(&b,&ctx,claim,None).is_err(),"an actual funded pairing still requires native preparation");
    if constraint {
        let mut disabled=active.clone();disabled.family_supplier_active=false;
        let off=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&disabled).unwrap();
        assert!(prepare_paired_original_v1(&off,&ctx,&object,Some(&fp.commitment)).is_err(),
            "a Constraint tail cannot bypass its bounded material supplier door");
    }
    let prepared=prepare_paired_original_v1(&b,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
    for (inventory,consent) in inventories.iter().zip(&consents) {
        let p=fs::derive_family_supplier_plan_v1(&family_state,&ctx,inventory,req.frame_v1().plan_v1().allocation_id,signed_material,consent).unwrap();
        let transfer=p.row.targets.iter().map(|t|t.packets.len()).sum::<usize>();
        eprintln!("native family calendar role={:?} native_span={} targets={} transfers={} holding={} total={} first={} hard={} ttl={}",consent.terms.role,inventory.native_inclusion_margin_v2(),p.row.targets.len(),transfer,p.packets.len()-transfer,p.packets.len(),ctx.daa_score,hard,crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().unwrap().draft_ttl_daa);
        for t in &p.row.targets {eprintln!("native family target={} maxbytes={} transfers={}",t.id,t.max_bytes,t.packets.len());}
    }
    // Dependent B6 FINAL native calendar facts, before the existing paid path.
    // Genuine signed TFO2/native family fixture; no new accepted Claim or point.
    {
        use prepared_final_service_facts_adapter_v11::NativePreparedFinalCalendarArchiveFactsV11 as Facts;
        use archive_service_view_v11::ActualArchiveActorRoleV11 as Role;
        let state_before=b.state.state_root();let journal_before=b.entries.len();
        let prepared_quote=prepare_paired_original_v1(&b,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        let facts=quote_prepared_original_final_archive_v11(&b,&ctx,claim,prepared_quote,|view|{
            // Local explicit checker only: no process-global reinitialization,
            // no mutation of already-held upstream leases or fake decode error.
            let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
                crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:0,database_bytes:0});
            assert!(matches!(Facts::with_checker_v11(view,&checker),Err(FundedOriginalRefusalV1::Resource(_))));
            assert_eq!(checker.outstanding(),(0,0));
            let facts=Facts::from_native_final_view_v11(view)?;
            let mut expected=Vec::new();
            for id in view.packet_ids_v11(){expected.push(view.packet_v11(id)?.clone());}
            assert_eq!(facts.packets_v11(),expected.as_slice(),"actual final book packet values, not precalendar concatenation");
            assert_eq!(facts.final_packet_vec_body_v11().canonical_bytes,borsh::object_length(&expected).unwrap() as u64);
            assert_eq!(facts.binding_body_v11().canonical_bytes,borsh::object_length(view.binding_v11()).unwrap() as u64);
            assert_eq!(facts.custody_body_v11().canonical_bytes,borsh::object_length(view.original_custody_v11()).unwrap() as u64);
            assert_eq!(facts.native_window_v11(),(ctx.daa_score,hard));
            assert_eq!(facts.scope_v11(),view.scope_v11());
            assert_eq!(facts.scope_v11().0,claim);assert_eq!(facts.scope_v11().1,req.frame_v1().plan_v1().source_id);
            assert_eq!(facts.scope_v11().2,req.frame_v1().plan_v1().allocation_id);
            assert_eq!(facts.scope_v11().5,req.source_graph_v1().unwrap().scope_v3());
            assert_eq!(facts.scope_v11().6,req.source_graph_v1().unwrap().absolute_scope_v3());
            assert_eq!(facts.actors_v11().iter().filter(|a|matches!(a.role,Role::FamilySupplier(_))).count(),consents.len());
            assert_eq!(facts.actors_v11().iter().filter(|a|matches!(a.role,Role::FamilySponsor(_))).count(),consents.len());
            assert_eq!(facts.logical_family_row_bodies_v11().len(),consents.len());
            for consent in &consents{
                let supplier=facts.actors_v11().iter().find(|a|a.role==Role::FamilySupplier(consent.terms.role)).unwrap();
                let sponsor=facts.actors_v11().iter().find(|a|a.role==Role::FamilySponsor(consent.terms.role)).unwrap();
                assert_eq!(supplier.bond,consent.terms.supplier);assert_eq!(sponsor.bond,consent.terms.sponsor);
                assert_eq!(supplier.existing_literal,sponsor.existing_literal,"two real roles can name one signed consent; do not duplicate stored body tariffs");
                assert_eq!(supplier.existing_literal.canonical_bytes,borsh::object_length(consent).unwrap() as u64);
            }
            assert_eq!(facts.work_v11().existing_canonical_serializer_invocations,
                facts.actors_v11().len() as u64+facts.logical_family_row_bodies_v11().len() as u64+3);
            assert_eq!(facts.missing_v11(),view.missing_actor_scope_v11(),"real uncovered obligations retained; no invented actor codec");
            include!("palw_tir_prepared_branch_obligations_native_test_v11.rs");
            assert!(facts.local_native_requests_max_v11()>view.native_requests_max_v11());
            Ok(facts)
        }).unwrap();
        assert!(!facts.packets_v11().is_empty());
        assert_eq!(b.state.state_root(),state_before);assert_eq!(b.entries.len(),journal_before);
        assert!(b.state.claim(&claim).is_none());assert!(b.state.funded_escrows_v1().values().all(|r|r.claim!=claim));
        drop(facts);
    }
    let coordinated=coordinate_funded_original_v1(&b,&ctx,claim,prepared).unwrap();
    assert_eq!(coordinated.family.len(),consents.len());
    assert!(family_state.family_supplier_rows_v1().unwrap().iter().all(|(_,r)|r.consent.terms.claim!=claim),"quote/coordinate installs no current-claim supplier row");
    // Missing X3 remains a named refusal even when clock v2 and the funded
    // service hard exist. Exercise the genuine object, paired checkpoint and
    // signed native original, not a synthetic claim or bypassed budget helper.
    if production&&source_history.is_none()&&!archive_finance {
        // A consumed Prefix without X3 is rejected even earlier by RFC0040.
        // The genuine prior source already exercises this financial negative.
        let missing = family_params.clone().with_claim_hard_deadline_v1(None);
        let mut rejected=TransitionBuilder::try_new(&family_state,&missing,false,false,false,false,&active).unwrap();
        let before=rejected.state.state_root();
        let why=super::super::objects::apply_object(&mut rejected,&ctx,&object).unwrap_err();
        assert!(matches!(why,PalwStateV2Error::BondBudgetRefused(ref reason) if reason.contains("X3 (palw_claim_hard_deadline_v1) does not govern")),"{why:?}");
        assert_eq!(rejected.state.state_root(),before);
        assert!(rejected.entries.is_empty(),"whole paired original must roll back before ordinary FP install");
        let expected=ctx.daa_score.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).unwrap();
        assert_eq!(super::super::palw_settlement_liability_fold_v1::v2_claim_signed_hard_deadline_v1(&family_params,ctx.daa_score),Ok(expected));
    }
    let declares_export=active.prefix_dependencies_v1.get(&claim).is_some_and(|e|!e.exports.is_empty());
    if production&&source_history.is_none()&&declares_export&&!archive_finance {
        // The old mechanics-only cap=1 cannot hold a genuine Q plus its E.
        // Use a separate untouched initial book; do not discard accepted source
        // reservations or substitute this refusal for the positive fold below.
        assert!(family_state.bond_budget.as_ref().unwrap().has_no_rows());
        let tiny=crate::palw_bond_budget_v1::PalwBondBudgetPolicyV1::unapproved_probe_v1();
        assert_eq!(tiny.max_open_claims_per_bond,1);
        let tiny_params=family_params.clone().with_bond_budget(Some(crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1{
            from_daa:0,policy:tiny.clone(),allocation:None}));
        let mut tiny_state=family_state.clone();
        let mut book=crate::palw_bond_budget_v1::PalwBondBudgetStateV1::new(ctx.daa_score,&tiny);
        book.activate_material_income_v3(&mut Vec::new());tiny_state.bond_budget=Some(book);
        let mut tiny_extras=active.clone();
        if let Some((policy,_))=tiny_extras.claim_local_rule_context_v1.as_mut(){
            let wire=borsh::to_vec(&tiny_params).unwrap();
            let digest=blake2b_simd::Params::new().hash_length(32).key(b"misaka/test/scoped-state-policy/v2").hash(&wire);
            policy.copy_from_slice(digest.as_bytes());
        }
        let mut rejected=TransitionBuilder::try_new(&tiny_state,&tiny_params,false,false,false,false,&tiny_extras).unwrap();
        rejected.claim_local_lock_point_v6=Some(ctx);
        let before=rejected.state.state_root();
        let why=super::super::objects::apply_object(&mut rejected,&ctx,&object).unwrap_err();
        assert!(matches!(why,PalwStateV2Error::BondBudgetExhausted{bond,ref why}
            if bond==producer&&why.contains("open-claim cap")),"genuine E+Q exceeds the tiny probe: {why:?}");
        assert_eq!(rejected.state.state_root(),before);
        assert!(rejected.entries.is_empty(),"paired refusal rolls back Q, E, supplier and financial writes");
        assert!(rejected.state.claim(&claim).is_none());
        assert!(rejected.state.prefix_export_reservation_v1(&ctx,claim).unwrap().is_none());
        assert!(rejected.state.bond_budget.as_ref().unwrap().has_no_rows());
    }
    let (accepted,accepted_delta)=apply_palw_transition_v2_with_extras(&family_state,&family_params,&ctx,
        std::slice::from_ref(&object),None,false,false,false,false,&active).unwrap();
    assert!(accepted.claim(&claim).is_some(),"real TFO2 public original must survive the production object fold");
    if declares_export {
        let e=accepted.prefix_export_reservation_v1(&ctx,claim).unwrap().unwrap();
        let book=accepted.bond_budget.as_ref().unwrap();
        let q=book.claim_row(&claim).unwrap();let erow=book.claim_row(&e.budget_id).unwrap();
        assert_ne!(e.budget_id,claim);
        assert_eq!((q.bond,erow.bond),(producer,producer));
        assert_eq!((q.reserved.claims,erow.reserved.claims),(1,1),"Q and E each consume their actual claim reservation");
        assert!(q.open&&erow.open&&q.liability_sompi>0&&erow.liability_sompi>0);
        assert!(book.bond_row(&producer).unwrap().open_claims>=2);
    }
    if production {
        let born=accepted.claim(&claim).unwrap();
        let expected=ctx.daa_score.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).unwrap();
        assert_eq!(crate::palw_court_clock_v2::palw_claim_x3_cap_hard_v2(&family_params,born),Some(expected));
        assert!(born.trace_retention_daa>=expected,"actual installed claim material covers its effective hard");
    }
    if let Some(history)=source_history.as_ref(){
        let reservation=accepted.prefix_export_reservation_v1(&history.point,history.claim).unwrap().unwrap();
        assert_eq!((reservation.source_claim,reservation.point),(history.claim,history.point));
        let cold=PalwStateCarriageV2::from_state(&accepted).into_state(&family_params,Some(accepted.state_root())).unwrap();
        let upstream=super::super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&cold,&family_params,history.claim,cold.claim(&history.claim).unwrap()).unwrap().unwrap();
        assert_eq!(upstream.prefix_export_v1(0).unwrap().unwrap().1,history.identity);
        let consumer=super::super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&cold,&family_params,claim,cold.claim(&claim).unwrap()).unwrap().unwrap();
        assert_eq!(consumer.prefix_use().unwrap().unwrap().intent.source,history.identity,"cold actual consumer retains exact actual prior export identity");
    }
    if archive_finance{
        let receipt=original_archive_reservation_proposal_v1::verified_original_archive_reservation_v1(&accepted,claim,ctx).unwrap().unwrap();
        assert!(receipt.constructed_v1()&&!receipt.released_v1());
        assert!(!active.original_archive_data_observer_v16);
        let f1=crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(ctx.block);
        assert!(!accepted.kernel_route_state_v1().unwrap().aux.contains_key(&(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1,f1.to_vec())));
    }
    if production && !prefix && !archive_finance && source_history.is_none() && source_export_request.is_none() {
        // Separate branch of the actual signed TFO2 admission above: no Claim,
        // service binding, phase or financial row is manufactured for this test.
        include!("palw_bound_original_operational_release_native_test_v43.rs");
    }
    if source_export_request.is_some(){
        // Return history only after the real signed native fold and its scoped
        // archive. Identity comes from the actual executed source view.
        let born=accepted.claim(&claim).unwrap();
        let origin=super::super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&accepted,&family_params,claim,born).unwrap().unwrap();
        let point=origin.accepting_point();assert_eq!(point,ctx);
        let (export,identity)=origin.prefix_export_v1(0).unwrap().unwrap();let export=export.clone();
        assert_eq!((identity.source_claim,identity.accepted_block,identity.accepted_daa,identity.accepted_blue_score),(claim,point.block,point.daa_score,point.blue_score));
        let restored=PalwStateCarriageV2::from_state(&accepted).into_state(&family_params,Some(accepted.state_root())).unwrap();
        let undone=super::super::revert_delta_v2(&accepted,&accepted_delta,&family_params).unwrap();
        assert_eq!(undone.state_root(),family_state.state_root());
        let replay=super::super::apply_delta_v2(&undone,&accepted_delta,&family_params).unwrap();
        assert_eq!(replay.state_root(),accepted.state_root());
        let cold=super::super::palw_claim_local_source_origin_v8::SourceOriginV8::for_claim(&restored,&family_params,claim,restored.claim(&claim).unwrap()).unwrap().unwrap();
        assert_eq!(cold.prefix_export_v1(0).unwrap().unwrap().1,identity);
        return Some(ScopedPrefixSourceHistoryV1{state:restored,params:family_params.clone(),point,claim,export,identity});
    }
    if archive_finance{
        include!("palw_original_archive_finance_native_test_v1.rs");
        return None;
    }
    if production {
        // The same actual original succeeds with finite production retirement.
        // Long trace retention alone, or a source for another claim, cannot
        // claim its staged/frame writer floor.
        let born=accepted.claim(&claim).unwrap();
        let native=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&active).unwrap();
        let prepared=prepare_paired_original_v1(&native,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        let source=admit_original_v1(&native,&ctx,claim,Some(prepared)).unwrap().unwrap();
        let actual=native.prospective_retirement_v1(claim,born,Some(&source)).unwrap();
        let expected=ctx.daa_score.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).unwrap();
        assert!(actual.floor_v1(&family_params,claim,born).unwrap()>=expected);
        let missing=native.prospective_retirement_v1(claim,born,None).unwrap();
        assert_eq!(missing.floor_v1(&family_params,claim,born).unwrap(),ctx.daa_score+3_000);
        let quanta=match &born.source {PalwClaimSourceV2::FreePrompt{quanta,..}=>*quanta,_=>panic!("actual FP")};
        let why=native.plan_free_prompt_budget_v1(&ctx,claim,born,quanta,&missing).unwrap_err();
        assert!(matches!(why,PalwStateV2Error::BondBudgetRefused(ref reason) if reason.contains("retire before its liability horizon")),"{why:?}");
        assert!(native.prospective_retirement_v1(h(0xdead),born,Some(&source)).is_err(),"foreign prepared original cannot lend retention");
        assert!(actual.floor_v1(&family_params,h(0xdead),born).is_err(),"token cannot be rebound");
        // Same producer and birth, genuinely prepared under a longer foreign
        // native configuration: its sealed witness still cannot lend a floor.
        let mut foreign_extras=active.clone();
        let mut foreign_config=foreign_extras.staged_public_court.unwrap();
        foreign_config.collection_daa=foreign_config.collection_daa.checked_add(actual.floor_v1(&family_params,claim,born).unwrap().checked_sub(ctx.daa_score).unwrap()).unwrap();
        foreign_extras.staged_public_court=Some(foreign_config);
        let foreign_builder=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&foreign_extras).unwrap();
        let mut foreign_claim=born.clone();
        let foreign_id=h(0xdead);
        let foreign=foreign_builder.prepare_staged_claim_for_budget_v1(foreign_id,&mut foreign_claim,None).unwrap();
        let witness=foreign.retirement_v1();
        assert_eq!((foreign_claim.bond,foreign_claim.accepted_daa),(born.bond,born.accepted_daa));
        assert!(witness.floor_v1(&family_params,foreign_id,&foreign_claim).unwrap()>actual.floor_v1(&family_params,claim,born).unwrap());
        assert!(witness.floor_v1(&family_params,claim,born).is_err(),"same-producer same-birth foreign prepared row is not authority");
        assert!(native.entries.is_empty());
    }
    if active.original_archive_data_observer_v16 {
        // Genuine same-point full-fold input basis: no fabricated Claim or aux.
        let mut input_read=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&active).unwrap();
        let missing=prepare_paired_original_v1(&input_read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        assert!(missing.input_footprint_v12.is_none(),"partial/pre-sign builder cannot invent future block input facts");
        input_read.claim_local_input_basis_v12=Some(NativeFoldInputBasisV12::from_native_fold_v12(&ctx,std::slice::from_ref(&object),PalwBlockWorkV3::None,&[],Hash64::default()).unwrap());
        let native=prepare_paired_original_v1(&input_read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        let footprint=native.input_footprint_v12.as_ref().unwrap();
        let origin=super::super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&accepted,claim,ctx).unwrap().unwrap();
        footprint.recheck_actual_v12(claim,ctx,origin.inputs()).unwrap();
        assert_native_input_footprint_actual_v12(footprint,claim,ctx,origin.inputs(),&object);
        assert!(input_read.entries.is_empty(),"codec footprint minting is read-only");
        include!("palw_claim_local_born_archive_quote_native_test_v12.rs");
        return None;
    }
// Genuine ordinary TFO2 transition above produced the initial native Claim.
// Reuse its real preclaim preparation; never construct a substitute Claim/ctx.
{
    let read=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&active).unwrap();
    let requirements=prepare_paired_original_v1(&read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap()
        .enrich_native_geometry_v1(&read,&ctx).unwrap().prepare_requirements_v1(&read,&ctx,
            PreparedRequirementBudgetsV1{numerical:budgets.0,input:budgets.1,source:budgets.2}).unwrap();
    let facts=requirements.quote_archive_adjuncts_v11(&read,&ctx,&object).unwrap();
    let actual=accepted_delta.entries.iter().find_map(|e|match e{
        PalwDeltaEntryV2::Claim{key,old:None,new:Some(c)}if *key==claim=>Some(c),_=>None,
    }).expect("ordinary successful FP fold has real initial Claim");
    let bound=facts.bodies().iter().find(|f|f.tag==3).unwrap().canonical_bytes_max;
    assert_eq!(borsh::object_length(actual).unwrap() as u64,bound,"fresh Claim bound is checked against actual ordinary native writer output");
    facts.recheck_actual_fresh_claim_v11(claim,actual,&ctx).unwrap();
    let context=accepted.staged_courts.get(&Key::CourtSharedContext(claim)).unwrap();
    assert_eq!(borsh::object_length(context).unwrap() as u64,facts.bodies().iter().find(|f|f.tag==9).unwrap().canonical_bytes_max);
    let table_key=crate::palw_court_family_material_v1::material_aux_key_v1(4,claim,None);
    let actual_table=accepted.kernel_route_state_v1().and_then(|r|r.aux.get(&(crate::palw_court_family_material_v1::FAMILY_MATERIAL_TABLE_V1,table_key)))
        .map(|bytes|borsh::from_slice::<crate::palw_court_family_material_v1::FamilyTableInventoryRowV1>(bytes).unwrap());
    if active.tir_funded_originals.get(&claim).unwrap().semantic_table_v2().is_some(){assert!(actual_table.is_some(),"genuine literal Some table was actually installed");}
    let n=borsh::object_length(&actual_table.as_ref()).unwrap() as u64;
    assert!(n<=facts.bodies().iter().find(|f|f.tag==17).unwrap().canonical_bytes_max,"real native Some/None table Option fits proposed quote without a fake grant");
}

    assert_eq!(accepted.family_supplier_rows_v1().unwrap().len(),family_state.family_supplier_rows_v1().unwrap().len()+consents.len(),"current original adds its exact roles without erasing prior source");
    for (id,row) in accepted.family_supplier_rows_v1().unwrap().iter().filter(|(_,r)|r.consent.terms.claim==claim){
        assert_eq!(signed_profile_generation_v3(&row.consent.terms).unwrap(),3);
        let key=family_carrier_projection_key_v3(*id);
        let birth:FamilyCarrierProjectionRowV3=borsh::from_slice(&accepted.kernel_route_state_v1().unwrap().aux[&(FAMILY_SUPPLIER_TABLE_V1,key.to_vec())]).unwrap();
        assert_eq!(birth.accepted_daa,ctx.daa_score);assert_eq!(birth.accepted_block,ctx.block);assert_eq!(birth.accepted_blue_score,ctx.blue_score);
        assert_eq!(birth.facts,public_quote.inventories_v2().iter().find(|i|i.role_v1()==row.consent.terms.role).unwrap().carrier_projection_v3().unwrap());
        accepted.check_family_carrier_birth_v3(row,&birth).unwrap();
        let mut wrong_birth=birth.clone();wrong_birth.accepted_blue_score+=1;
        assert!(accepted.check_family_carrier_birth_v3(row,&wrong_birth).is_err(),"exact source birth tuple cannot be rewritten");
        if !scoped {
            let mut retired=accepted.clone();retired.claims.remove(&claim);
            let missing=retired.check_family_carrier_birth_v3(row,&birth).unwrap_err();
            assert!(matches!(missing,PalwStateV2Error::NativeResource(_)),"frame/funding without exact birth source is unavailable, never an alias: {missing:?}");
        }

        let mut bad=accepted.clone();let mut changed=birth.clone();changed.facts.coordinates=h(0x43999);
        bad.kernel_route.as_mut().unwrap().aux.insert((FAMILY_SUPPLIER_TABLE_V1,key.to_vec()),borsh::to_vec(&changed).unwrap());
        assert!(fs::validate_family_suppliers_v1(&bad,&family_params).is_err(),"stored facts cannot rewrite a signed inventory or fee quote");
    }
    {
        let mut changed=actual.carrier().clone();changed.inputs[0].previous_outpoint.transaction_id=h(0x43998);changed.finalize();
        let mut wrong=active.clone();wrong.tir_funded_originals.insert(claim,palw_funded_original_pairing_payload_v1(&changed,&wire).unwrap());
        let mut trial=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&wrong).unwrap();
        let root=trial.state.state_root();let entries=borsh::to_vec(&trial.entries).unwrap();
        let error=super::super::objects::apply_object(&mut trial,&ctx,&object).unwrap_err();
        assert!(matches!(error,PalwStateV2Error::CarriageInconsistent(ref why) if why=="family supplier consent differs from native inventory, policy or acceptance interval"),"actual input slice must match the quote's signed native digest: {error:?}");
        assert_eq!(trial.state.state_root(),root);assert_eq!(borsh::to_vec(&trial.entries).unwrap(),entries);
    }
    let accepted_claim=accepted.claim(&claim).unwrap();
    assert_eq!(accepted_claim.job_identity,crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment));
    accepted.check_public_fp_job_v1(&claim,accepted_claim,&fp.commitment).unwrap();
    if let Some(table)=wrapper.semantic_table.as_ref() {
        for (replacement,reason) in [
            (None,"family allocation requires literal authenticated TFO2 table"),
            (Some({let mut bytes=table.clone();bytes[0]^=1;bytes}),"family token table magic"),
        ] {
            let mut bad_wrapper=wrapper.clone();bad_wrapper.semantic_table=replacement;
            let bad_wire=bad_wrapper.encode_v2().unwrap();let mut bad_tx=pairing.carrier().clone();let mut invalid=active.clone();
            bad_tx.payload=if let Some(provenance)=invalid.prefix_dependencies_v1.get_mut(&claim){provenance.inner=bad_wire.clone();provenance.encode_v1().unwrap()}else{bad_wire.clone()};bad_tx.finalize();
            invalid.tir_funded_originals.insert(claim,palw_funded_original_pairing_payload_v1(&bad_tx,&bad_wire).unwrap());
            let mut trial=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&invalid).unwrap();
            let before=trial.state.state_root();let entries=borsh::to_vec(&trial.entries).unwrap();
            let error=super::super::objects::apply_object(&mut trial,&ctx,&object).unwrap_err();
            assert!(matches!(error,PalwStateV2Error::CarriageInconsistent(ref why) if why==reason),"exact table refusal only, got {error:?}");
            assert_eq!(trial.state.state_root(),before,"actual object checkpoint rolls back every refused table write");
            assert_eq!(borsh::to_vec(&trial.entries).unwrap(),entries);
            assert!(trial.state.claim(&claim).is_none()&&trial.state.family_supplier_rows_v1().unwrap().iter().all(|(_,r)|r.consent.terms.claim!=claim));
            assert_eq!(trial.state.bond(&producer).unwrap().collateral,family_state.bond(&producer).unwrap().collateral);
        }
    }
    {
        let mut short=family_state.clone();
        let read=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&active).unwrap();
        let held=super::super::palw_court_capacity_fold_v1::fee_reserved_principal_v1(&read,&producer,ctx.daa_score).unwrap();
        short.bonds.get_mut(&producer).unwrap().collateral=u64::try_from(held).unwrap().checked_add(total_fees).and_then(|n|n.checked_add(u64::try_from(total_capital).unwrap())).unwrap()-1;
        let read=TransitionBuilder::try_new(&short,&family_params,false,false,false,false,&active).unwrap();
        let prepared=prepare_paired_original_v1(&read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        let failure=coordinate_funded_original_v1(&read,&ctx,claim,prepared).err().unwrap();
        assert!(format!("{failure:?}").contains("insufficient uncommitted real principal"),"{failure:?}");
        let mut trial=TransitionBuilder::try_new(&short,&family_params,false,false,false,false,&active).unwrap();
        let before=trial.state.state_root();let entries=borsh::to_vec(&trial.entries).unwrap();
        let error=super::super::objects::apply_object(&mut trial,&ctx,&object).unwrap_err();
        assert!(matches!(error,PalwStateV2Error::CarriageInconsistent(ref why) if why=="family sponsor/supplier has insufficient uncommitted real principal"),"exact family funding refusal only, got {error:?}");
        assert_eq!(trial.state.state_root(),before);assert_eq!(borsh::to_vec(&trial.entries).unwrap(),entries);
        assert!(trial.state.claim(&claim).is_none());assert!(palw_tir_public_claim_v1(&trial.state,claim).is_none());
        assert!(trial.state.family_supplier_rows_v1().unwrap().iter().all(|(_,r)|r.consent.terms.claim!=claim));assert!(trial.state.funded_escrows_v1().values().all(|r|r.claim!=claim));
        assert_eq!(trial.state.bond(&producer).unwrap().collateral,short.bond(&producer).unwrap().collateral,
            "several roles of one actor cannot spend its principal independently or leave a partial original");
    }
    {
        let mut too_short=consents.clone();let mut terms=too_short[0].terms.clone();terms.respond_until_daa-=1;
        too_short[0]=sign_family_supplier_v1(terms,|_,message,context|Some(mldsa::sign(&kp.signing_key,message,context,seed).unwrap().as_ref().to_vec())).unwrap();
        let outer=crate::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2{version:3,
            funded_original:pairing.envelope().encode_v1().unwrap(),suppliers:too_short,semantic_table:wrapper.semantic_table.clone()};
        let bad_wire=outer.encode_v2().unwrap();let mut tx=pairing.carrier().clone();let mut bad=active.clone();
        tx.payload=if let Some(provenance)=bad.prefix_dependencies_v1.get_mut(&claim){provenance.inner=bad_wire.clone();provenance.encode_v1().unwrap()}else{bad_wire.clone()};tx.finalize();
        bad.tir_funded_originals.insert(claim,palw_funded_original_pairing_payload_v1(&tx,&bad_wire).unwrap());
        let mut trial=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&bad).unwrap();
        let before=trial.state.state_root();let entries=borsh::to_vec(&trial.entries).unwrap();
        let error=super::super::objects::apply_object(&mut trial,&ctx,&object).unwrap_err();
        assert!(matches!(error,PalwStateV2Error::CarriageInconsistent(ref why) if why=="family supplier consent differs from native inventory, policy or acceptance interval"),"exact native consent horizon refusal only, got {error:?}");
        assert_eq!(trial.state.state_root(),before);assert_eq!(borsh::to_vec(&trial.entries).unwrap(),entries);
        assert!(trial.state.claim(&claim).is_none()&&trial.state.family_supplier_rows_v1().unwrap().iter().all(|(_,r)|r.consent.terms.claim!=claim),
            "a valid signature on too short an original hard horizon never reaches FP or financial install");
        assert!(palw_tir_public_claim_v1(&trial.state,claim).is_none()&&trial.state.funded_escrows_v1().values().all(|r|r.claim!=claim));
        assert_eq!(trial.state.bond(&producer).unwrap().collateral,family_state.bond(&producer).unwrap().collateral);
    }
    for (id,row) in accepted.family_supplier_rows_v1().unwrap().iter().filter(|(_,r)|r.consent.terms.claim==claim) {
        let packets=row.targets.iter().map(|t|t.packets.len() as u64).sum();
        let geometry=fs::family_finance_geometry_v2(row.targets.len() as u64,packets).unwrap();
        eprintln!("[family-finance-geometry] role={:?} targets={} materialPackets={} retainedBytes={} fullJournalBytes={} queryHeapBytes={}",row.consent.terms.role,row.targets.len(),packets,geometry.retained,geometry.journal,geometry.reconstruction);
        let header=&accepted.kernel_route().unwrap().aux[&(FAMILY_SUPPLIER_TABLE_V1,id.as_byte_slice().to_vec())];
        let immutable:FamilySupplierRowV1=borsh::from_slice(header).unwrap();
        assert!(immutable.completed.is_empty()&&immutable.defaulted_daa.is_none()&&immutable.fee_state==FamilySupplierFeeStateV1::Held);
        let actual:usize=accepted.kernel_route().unwrap().aux.iter().filter(|((t,k),_)|*t==FAMILY_SUPPLIER_TABLE_V1&&
            (k.as_slice()==id.as_byte_slice()||(k.len()>=65&& &k[1..65]==id.as_byte_slice())))
            .map(|((_,key),value)|key.len()+value.len()+9).sum();
        let inventory=public_quote.inventories_v2().iter().find(|i|i.role_v1()==row.consent.terms.role).unwrap();
        let priced=fs::derive_family_supplier_plan_v1(&family_state,&ctx,inventory,row.allocation,signed_material,&row.consent).unwrap();
        let held=priced.packets.iter().filter(|p|p.cost.io==0).try_fold(0u64,|n,p|n.checked_add(accepted.court_service_state_v1().unwrap().book.packet(&p.id).unwrap().cost.bytes)).unwrap();
        assert!(held>=actual as u64&&geometry.journal>geometry.retained,"actual charged holding includes new3 birth/QM metadata; no held byte is a material transfer");
        let mut corrupt=accepted.clone();corrupt.kernel_route.as_mut().unwrap().aux.remove(&(FAMILY_SUPPLIER_TABLE_V1,family_status_key_v2(*id).to_vec()));
        assert!(corrupt.family_supplier_rows_v1().is_err(),"missing normalized finance status is not a Held/default-free fallback");
        let alien=h(0x421234);let orphan=FamilySupplierCompletionV2{version:2,contract:alien,target:row.targets[0].id,digest:h(0x421235)};
        let mut corrupt=accepted.clone();corrupt.kernel_route.as_mut().unwrap().aux.insert((FAMILY_SUPPLIER_TABLE_V1,family_completion_key_v2(alien,orphan.target).to_vec()),borsh::to_vec(&orphan).unwrap());
        assert!(corrupt.family_supplier_rows_v1().is_err(),"orphan completion cannot acquire authority from a bare digest");
        if let Some(target)=row.targets.iter().find(|t|!row.completed.contains_key(&t.id)) {
            let bare=FamilySupplierCompletionV2{version:2,contract:*id,target:target.id,digest:h(0x421236)};
            let mut corrupt=accepted.clone();corrupt.kernel_route.as_mut().unwrap().aux.insert(
                (FAMILY_SUPPLIER_TABLE_V1,family_completion_key_v2(*id,bare.target).to_vec()),borsh::to_vec(&bare).unwrap());
            let error=fs::validate_family_suppliers_v1(&corrupt,&family_params).unwrap_err();
            assert!(error.to_string().contains("completed target lost actual retained body"),"a target digest without authenticated supply cannot restore: {error}");
        }
    }
    let mut all_material_ready=ctx.daa_score+1;
    for (id,c) in accepted.family_supplier_rows_v1().unwrap().iter().filter(|(_,r)|r.consent.terms.claim==claim) {
        let mut chosen=Vec::new();
        for target in &c.targets {
            let key=family_primary_window_key_v2(*id,target.id);
            let w:FamilyPrimaryWindowV2=borsh::from_slice(&accepted.kernel_route_state_v1().unwrap().aux[&(FAMILY_SUPPLIER_TABLE_V1,key.to_vec())]).unwrap();
            let ttl=crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().unwrap().draft_ttl_daa;
            assert!((target.packets.len() as u64)*w.span_daa<=ttl);
            all_material_ready=all_material_ready.max(w.first_daa+(target.packets.len() as u64)*w.span_daa);
            for(i,p)in target.packets.iter().enumerate(){let p=accepted.court_service_state_v1().unwrap().book.packet(p).unwrap();assert_eq!(p.release_daa,w.first_daa+(i as u64)*w.span_daa);assert_eq!(p.service_before,p.release_daa+1);}
            chosen.push((key,w));
        }
        if let Some((key,w))=chosen.first(){
            let mut invalid=accepted.clone();let mut changed=w.clone();changed.span_daa=0;
            invalid.kernel_route.as_mut().unwrap().aux.insert((FAMILY_SUPPLIER_TABLE_V1,key.to_vec()),borsh::to_vec(&changed).unwrap());
            assert!(fs::validate_family_suppliers_v1(&invalid,&family_params).is_err(),"birth calendar cannot restore with a changed clock/part order");
            assert!(PalwStateCarriageV2::from_state(&invalid).into_state(&family_params,None).is_err());
        }
    }
    for original in derivation.signer_packets.iter().chain(&derivation.source_packets) {
        let p=accepted.court_service_state_v1().unwrap().book.packet(&original.id).unwrap();
        assert_eq!(p.release_daa,original.release_daa.max(all_material_ready),"no existing Court/source opportunity precedes required new material Finish");
        assert_eq!(p.service_before,original.service_before,"the material barrier never extends or shortens a signed early deadline");
        assert!(p.release_daa<p.service_before);
    }
    let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&accepted)).unwrap();
    let reloaded=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&family_params,Some(accepted.state_root())).unwrap();
    assert_eq!(accepted.state_root(),reloaded.state_root());
    {
        let mut corrupt=accepted.clone();let mut service=accepted.court_service_state_v1().unwrap().clone();
        service.book.packets.get_mut(&derivation.retention_packet.id).unwrap().cost.work+=1;
        let changed=if let Some(released)=accepted.court_service_row_v3_v1(){let mut row=released.clone();row.state=service;Row::CourtServiceV3(Box::new(row).into())}
            else{Row::CourtService(Box::new(service).into())};
        corrupt.staged_courts.insert(Key::CourtService,changed);
        let error=fs::validate_family_suppliers_v1(&corrupt,&family_params).unwrap_err();
        assert!(error.to_string().contains("family restored original packet"),"native aggregate restore detects changed original retention cost independently of generic book consistency: {error}");
        assert!(PalwStateCarriageV2::from_state(&corrupt).into_state(&family_params,None).is_err());
    }

    if wrapper.semantic_table.is_some() {
        use crate::palw_court_family_material_v1 as mat;
        let key=(mat::FAMILY_MATERIAL_TABLE_V1,mat::material_aux_key_v1(4,claim,None));
        let original:mat::FamilyTableInventoryRowV1=borsh::from_slice(&accepted.kernel_route.as_ref().unwrap().aux[&key]).unwrap();
        assert!(original.allocation_rules_v2.is_some(),"native installer retains actual original policy context");
        for missing in [false,true] {
            let mut row=original.clone();
            if missing {row.allocation_rules_v2=None;} else {row.allocation_rules_v2.as_mut().unwrap().dissection^=true;}
            let mut corrupt=accepted.clone();corrupt.kernel_route.as_mut().unwrap().aux.insert(key.clone(),borsh::to_vec(&row).unwrap());
            assert!(fs::validate_family_suppliers_v1(&corrupt,&family_params).is_err(),"missing/substituted native original rule context cannot restore through valid table bytes");
            assert!(PalwStateCarriageV2::from_state(&corrupt).into_state(&family_params,None).is_err());
        }
    }

    let mut undone=accepted.clone();
    for entry in accepted_delta.entries.iter().rev(){apply_delta_entry(&mut undone,entry,true).unwrap();}
    // State point belongs to the enclosing block; restore the parent point as the real delta replay does.
    undone.set_last_point_for_tests(family_state.last_point().cloned());
    assert_eq!(undone.state_root(),family_state.state_root());
    include!("palw_funded_original_settlement_native_test_block.rs");
    include!("palw_court_family_owner_clock_v3_test_block.rs");
    include!("palw_court_family_material_production_test_block.rs");
    let dormant=family_params.clone().with_family_supplier_from(None);
    let b=TransitionBuilder::try_new(&family_state,&dormant,false,false,false,false,&active).unwrap();
    if let Ok(Some(prepared))=prepare_paired_original_v1(&b,&ctx,&object,Some(&fp.commitment)) {
        assert!(admit_original_v1(&b,&ctx,claim,Some(prepared)).is_err(),"new TFO2 stays dormant despite old active fences");
    }
    assert!(palw_family_original_native_quote_v3(&family_state,&dormant,&ctx,&active,&object,wrapper.semantic_table.as_deref(),&actual.carrier().inputs).is_err(),
        "owning fence is mandatory before native family quote, independently of legacy allocation refusal");
    if !production {
        let legacy=TransitionBuilder::try_new(&served,&params,false,false,false,false,&ok).unwrap();
        let prepared=prepare_paired_original_v1(&legacy,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        assert!(admit_original_v1(&legacy,&ctx,claim,Some(prepared)).is_ok(),"old TFO1 admission is preserved below the new fence");
    }
    let prepared=prepare_paired_original_v1(&native,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
    assert!(admit_original_v1(&native,&ctx,claim,Some(prepared)).is_err(),"active new cohort cannot bypass suppliers with TFO1");

    let (_,mut service)=funded_book_candidate_v1(&native,&req,&derivation,&admission,&req.frame_v1().custody_v1().liable_signers_v1()).ok().unwrap();
    fs::reserve_family_plans_v1(&mut service,claim,&mut[make_plan()],graph).unwrap();
    for target in &make_plan().row.targets {for p in &target.packets{assert!(service.claims[&claim].packet_ids.contains(p));}}
    let monetary_parent=accepted.clone();
    for supplied in [false,true] {
        let mut parent=monetary_parent.clone();
        if supplied {
            let mut supplied_book=parent.court_service_state_v1().unwrap().clone();
            let units:std::collections::BTreeSet<_>=make_plan().row.targets.iter().flat_map(|t|&t.packets).map(|id|supplied_book.book.assigned_daa(id).unwrap()).collect();
            for unit in units {supplied_book.book.observe_block(unit,h(unit).as_bytes(),crate::palw_court_service_v1::ServiceCost::default()).unwrap();}
            parent.staged_courts.insert(Key::CourtService,Row::CourtService(Box::new(supplied_book).into()));
        }
        let mut ex=active.clone();ex.court_service_inclusion_premise_v1=crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_INCLUSION_PREMISE_V1;
        let mut money=TransitionBuilder::try_new(&parent,&family_params,false,false,false,false,&ex).unwrap();
        let before=family_state.bond(&producer).unwrap().collateral;
        let saved=money.try_checkpoint().unwrap();
        assert_eq!(money.state.bond(&producer).unwrap().collateral,before-total_fees);
        assert_eq!(money.state.family_supplier_collateral_reserved_v1(&producer,ctx.daa_score),total_capital);
        let at=if supplied {
            let book=&parent.court_service_state_v1().unwrap().book;
            make_plan().row.targets.iter().map(|t|t.packets.iter().map(|id|book.assigned_daa(id).unwrap()).max().unwrap())
                .min().unwrap()+crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.service_span_daa+1
        }else{hard+1};
        if supplied{assert!(at<=hard,"timely default fixture follows real native reserved slot, without shortening a deadline");}
        let now=PalwBlockContextV2{daa_score:at,block:h(at),blue_score:at,subsidy:0};
        let (advanced,_)=apply_palw_transition_v2_with_extras(&parent,&family_params,&now,&[],None,false,false,false,false,&ex).unwrap();
        money.state=advanced.clone();
        let row=fs::family_contract_for_material_v1(&money.state,contract).unwrap().unwrap();
        assert_eq!(row.defaulted_daa.is_some(),supplied,"missing service opportunity is never a supplier default");
        let FamilySupplierFeeStateV1::Queued{queue,..}=row.fee_state else{panic!("fee must be physically queued")};
        let burned=if supplied {terms.collateral_sompi}else{0};
        assert!(money.state.bond(&producer).unwrap().collateral+money.state.pending_payouts[&queue].amount+burned<=before,
            "family funding cannot mint real collateral principal (ordinary native claim charges may also burn)");
        let bytes=borsh::to_vec(&PalwStateCarriageV2::from_state(&advanced)).unwrap();
        let restored=borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&family_params,Some(advanced.state_root())).unwrap();
        assert_eq!(restored.state_root(),advanced.state_root(),"supplier default/retiring actor and terminal original survive cold restore");
        let once=money.state.clone();fs::tick_family_suppliers_v1(&mut money,&now).unwrap();assert_eq!(money.state,once);
        money.write_payout(queue,None).unwrap();fs::acknowledge_family_fee_payments_v1(&mut money,&now).unwrap();
        assert!(matches!(fs::family_contract_for_material_v1(&money.state,contract).unwrap().unwrap().fee_state,FamilySupplierFeeStateV1::Paid{..}));
        money.restore(saved);assert_eq!(money.state,parent,"rollback restores source collateral, held fee, service charge, queue and once markers together");
    }
    include!("palw_court_family_supplier_exit_native_test_block.rs");
}
