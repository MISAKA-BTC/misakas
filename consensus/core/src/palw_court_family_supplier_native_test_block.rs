// Included in the real native funded-original fixture. Native preparation and
// signatures below are real; the monetary subtransition fixture is deliberately
// not presented as an accepted block or multi-node campaign.
{
    use crate::palw_court_family_supplier_v1::*;
    use super::super::palw_court_family_supplier_fold_v1 as fs;
    let mut family_state=served.clone();
    let network=extras.tir_public_network_domain_v1.unwrap();
    let ruleset=h(0x42);
    family_state.kernel_route=Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(network,ruleset),None,
        misaka_palw_kernel::rows::LedgerScalarsV1{daa:ctx.daa_score,burned:0}));
    let budget_policy=crate::palw_bond_budget_v1::PalwBondBudgetPolicyV1::unapproved_probe_v1();
    let mut book=crate::palw_bond_budget_v1::PalwBondBudgetStateV1::new(ctx.daa_score,&budget_policy);
    book.activate_material_income_v3(&mut Vec::new());family_state.bond_budget=Some(book);
    let family_params=params.clone().with_settlement_once_from(Some(0)).with_family_supplier_from(Some(0))
        .with_material_income_from(Some(0)).with_bond_budget(Some(crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1{
            from_daa:0,policy:budget_policy,allocation:None}));
    let mut family_extras=ok.clone();family_extras.family_supplier_active=true;family_extras.staged_tir_route_active=true;
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
    let inventories=roles.iter().map(|role|crate::palw_court_family_material_v1::derive_family_inventory_with_table_v1(
        &family_state,req.frame_v1(),req.geometry.source_v1(),graph,profile,&program,req.inputs_v1(),family_semantics.as_ref(),*role,checked_table.as_ref()).unwrap()).collect::<Vec<_>>();
    let signed_material=crate::palw_court_service_state_v2::tir_service_required_material_until_v1(req.frame_v1(),hard).unwrap();
    let consents=inventories.iter().enumerate().map(|(index,inventory)|{
        let offered=FamilySupplierTermsV1{version:1,network,ruleset,claim,source:inventory.source_v1(),relative_graph:inventory.relative_graph_v1(),
            role:inventory.role_v1(),inventory:inventory.id_v1(),supplier:producer,sponsor:producer,
            max_material_bytes:inventory.max_material_bytes_v1(),accept_from_daa:ctx.daa_score,accept_until_daa:ctx.daa_score,
            respond_until_daa:hard,material_until_daa:signed_material,fee_profile:h(1),capital_profile:h(1),retention_profile:h(1),settlement_policy:h(1),
            fee_sompi:1,collateral_sompi:1,nonce:h(0x4201+index as u64)};
        let terms=fs::palw_family_supplier_terms_quote_v1(inventory,offered).unwrap();
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
        version:2,funded_original:pairing.envelope().encode_v1().unwrap(),suppliers:consents.clone(),semantic_table:family_table.as_ref().map(crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1)};
    let wire=wrapper.encode_v2().unwrap();
    let decoded=crate::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2::decode_v2(&wire).unwrap();
    assert_eq!(decoded,wrapper);
    let mut tx=pairing.carrier().clone();tx.payload=wire;
    let actual=palw_funded_original_pairing_v1(&tx).unwrap();
    assert_eq!(actual.suppliers_v2(),consents.as_slice());assert_ne!(actual.outpoint(),pairing.outpoint());
    if production {if let Ok(dir)=std::env::var("MISAKA_FAMILY_NATIVE_FIXTURE_DIR"){
        let dir=std::path::Path::new(&dir);std::fs::create_dir_all(dir).unwrap();
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
        std::fs::write(dir.join("PROVENANCE.txt"),"Genuine reexecuted native family6prompt/1decode/vocab2 fixture; fixture-kind.txt identifies source/consumer. Prestate is explicitly seeded, not a network bootstrap. Public TEST executor/supplier seed in executor-seed.borsh (default0x6d or selectednativecard); funding payer0x7a, outsider0x6e. Pinned original/family schedules, native Source/graph/table/EOF inventories, c=1/DAA, no prior CourtService row. Node integration must regenerate/resign native policy scopes using its own accepted point/network before signing real UTXO inputs.\n").unwrap();
    }}

    let mut active=family_extras.clone();active.tir_funded_originals.insert(claim,actual.clone());
    let b=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&active).unwrap();
    let prepared=prepare_paired_original_v1(&b,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
    let coordinated=coordinate_funded_original_v1(&b,&ctx,claim,prepared).unwrap();
    assert_eq!(coordinated.family.len(),consents.len());
    assert!(family_state.family_supplier_rows_v1().unwrap().is_empty(),"quote/coordinate does not debit principal");
    if let Some(table)=wrapper.semantic_table.as_ref() {
        for replacement in [None,Some({let mut bytes=table.clone();bytes[0]^=1;bytes})] {
            let mut bad_wrapper=wrapper.clone();bad_wrapper.semantic_table=replacement;
            let mut bad_tx=pairing.carrier().clone();bad_tx.payload=bad_wrapper.encode_v2().unwrap();
            let mut invalid=active.clone();invalid.tir_funded_originals.insert(claim,palw_funded_original_pairing_v1(&bad_tx).unwrap());
            let (rejected,_)=apply_palw_transition_v2_with_extras(&family_state,&family_params,&ctx,std::slice::from_ref(&object),None,false,false,false,false,&invalid).unwrap();
            assert!(rejected.claim(&claim).is_none()&&rejected.family_supplier_rows_v1().unwrap().is_empty(),
                "missing or root-inconsistent exact table inventory never admits the native original");
            assert_eq!(rejected.bond(&producer).unwrap().collateral,family_state.bond(&producer).unwrap().collateral);
        }
    }

    let (accepted,accepted_delta)=apply_palw_transition_v2_with_extras(&family_state,&family_params,&ctx,
        std::slice::from_ref(&object),None,false,false,false,false,&active).unwrap();
    assert!(accepted.claim(&claim).is_some(),"real TFO2 public original must survive the production object fold");
    assert_eq!(accepted.family_supplier_rows_v1().unwrap().len(),consents.len(),"real FP/frame/reservations/principal/consent commit together");
    {
        let mut short=family_state.clone();
        short.bonds.get_mut(&producer).unwrap().collateral=total_fees.checked_add(u64::try_from(total_capital).unwrap()).unwrap()-1;
        let read=TransitionBuilder::try_new(&short,&family_params,false,false,false,false,&active).unwrap();
        let prepared=prepare_paired_original_v1(&read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        let failure=coordinate_funded_original_v1(&read,&ctx,claim,prepared).err().unwrap();
        assert!(format!("{failure:?}").contains("insufficient uncommitted real principal"),"{failure:?}");
        let (refused,_)=apply_palw_transition_v2_with_extras(&short,&family_params,&ctx,std::slice::from_ref(&object),None,false,false,false,false,&active).unwrap();
        assert!(refused.claim(&claim).is_none());assert!(palw_tir_public_claim_v1(&refused,claim).is_none());
        assert!(refused.family_supplier_rows_v1().unwrap().is_empty());assert!(refused.funded_escrows_v1().is_empty());
        assert_eq!(refused.bond(&producer).unwrap().collateral,short.bond(&producer).unwrap().collateral,
            "several roles of one actor cannot spend its principal independently or leave a partial original");
    }
    {
        let mut too_short=consents.clone();let mut terms=too_short[0].terms.clone();terms.respond_until_daa-=1;
        too_short[0]=sign_family_supplier_v1(terms,|_,message,context|Some(mldsa::sign(&kp.signing_key,message,context,seed).unwrap().as_ref().to_vec())).unwrap();
        let outer=crate::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2{version:2,
            funded_original:pairing.envelope().encode_v1().unwrap(),suppliers:too_short,semantic_table:wrapper.semantic_table.clone()};
        let mut tx=pairing.carrier().clone();tx.payload=outer.encode_v2().unwrap();
        let mut bad=active.clone();bad.tir_funded_originals.insert(claim,palw_funded_original_pairing_v1(&tx).unwrap());
        let (rejected,_)=apply_palw_transition_v2_with_extras(&family_state,&family_params,&ctx,std::slice::from_ref(&object),None,false,false,false,false,&bad).unwrap();
        assert!(rejected.claim(&claim).is_none()&&rejected.family_supplier_rows_v1().unwrap().is_empty(),
            "a valid signature on too short an original hard horizon never reaches FP or financial install");
    }
    for (id,row) in accepted.family_supplier_rows_v1().unwrap().iter() {
        let packets=row.targets.iter().map(|t|t.packets.len() as u64).sum();
        let geometry=fs::family_finance_geometry_v2(row.targets.len() as u64,packets).unwrap();
        eprintln!("[family-finance-geometry] role={:?} targets={} materialPackets={} retainedBytes={} fullJournalBytes={} queryHeapBytes={}",row.consent.terms.role,row.targets.len(),packets,geometry.retained,geometry.journal,geometry.reconstruction);
        let header=&accepted.kernel_route().unwrap().aux[&(FAMILY_SUPPLIER_TABLE_V1,id.as_byte_slice().to_vec())];
        let immutable:FamilySupplierRowV1=borsh::from_slice(header).unwrap();
        assert!(immutable.completed.is_empty()&&immutable.defaulted_daa.is_none()&&immutable.fee_state==FamilySupplierFeeStateV1::Held);
        let actual:usize=accepted.kernel_route().unwrap().aux.iter().filter(|((t,k),_)|*t==FAMILY_SUPPLIER_TABLE_V1&&
            (k.as_slice()==id.as_byte_slice()||(k.len()>=65&& &k[1..65]==id.as_byte_slice())))
            .map(|((_,key),value)|key.len()+value.len()+9).sum();
        assert!(geometry.retained>=actual as u64&&geometry.journal>geometry.retained);
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
    include!("palw_court_family_owner_clock_v3_test_block.rs");
    include!("palw_court_family_material_production_test_block.rs");
    let dormant=family_params.clone().with_family_supplier_from(None);
    let b=TransitionBuilder::try_new(&family_state,&dormant,false,false,false,false,&active).unwrap();
    if let Ok(Some(prepared))=prepare_paired_original_v1(&b,&ctx,&object,Some(&fp.commitment)) {
        assert!(admit_original_v1(&b,&ctx,claim,Some(prepared)).is_err(),"new TFO2 stays dormant despite old active fences");
    }
    assert!(palw_family_original_native_quote_v2(&family_state,&dormant,&ctx,&active,&object,wrapper.semantic_table.as_deref()).is_err(),
        "owning fence is mandatory before native family quote, independently of legacy allocation refusal");
    if !production {
        let legacy=TransitionBuilder::try_new(&served,&params,false,false,false,false,&ok).unwrap();
        let prepared=prepare_paired_original_v1(&legacy,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        assert!(admit_original_v1(&legacy,&ctx,claim,Some(prepared)).is_ok(),"old TFO1 admission is preserved below the new fence");
    }
    let prepared=prepare_paired_original_v1(&native,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
    assert!(admit_original_v1(&native,&ctx,claim,Some(prepared)).is_err(),"active new cohort cannot bypass suppliers with TFO1");

    let (_,mut service)=funded_book_candidate_v1(&native,&req,&derivation,&admission,&req.frame_v1().custody_v1().liable_signers_v1()).ok().unwrap();
    fs::reserve_family_plans_v1(&mut service,claim,&[make_plan()]).unwrap();
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
