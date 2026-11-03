// Actual tiny native originals/registered programs/complete root covers. The fixture's
// original custody acceptance is explicit; these tests do not claim a mined genesis.
#[test]
fn family_material_native_parameter_inventory_authenticates_exact_root_inputs_and_prompt() {
    use crate::palw_court_family_material_v1 as m;
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    use crate::palw_court_family_supplier_v1::FamilySupplierRoleV1 as Role;
    use crate::palw_court_shared_material_intake_v1::MaterialTargetV1 as T;
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_material_frame_fixture_v1(true);
    let schedule = material_test_schedule();
    let (state, _, _, claim, _) = material_intake_world(&fx);
    let before = state.state_root();
    let inventory = m::derive_current_family_inventory_v1(&state, claim, Role::ParameterAndPrompt).unwrap();
    let leaves = crate::palw_court_shared_material_intake_v1::required_parameter_leaves_v1(&state, claim, schedule.cover).unwrap();
    let prompt = m::FamilyMaterialTargetV1::Input(T::PromptFlat);
    assert_eq!(inventory.targets_v1().len(), leaves.len() + 1);
    for leaf in leaves {
        let target = m::FamilyMaterialTargetV1::Input(T::Parameter { leaf });
        let bytes = borsh::to_vec(&fx.execution.param_opening(leaf).unwrap()).unwrap();
        assert!(bytes.len() as u64 <= inventory.target_bound_v1(target).unwrap());
        assert!(m::authenticate_family_material_v1(&state, &inventory, target, &bytes).is_ok());
        let mut corrupted = bytes.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(m::authenticate_family_material_v1(&state, &inventory, target, &corrupted).is_err());
    }
    let body = borsh::to_vec(&fx.execution.prompt).unwrap();
    assert!(m::authenticate_family_material_v1(&state, &inventory, prompt, &body).is_ok());
    let mut changed = fx.execution.prompt.clone();
    changed[0] ^= 1;
    assert!(m::authenticate_family_material_v1(&state, &inventory, prompt, &borsh::to_vec(&changed).unwrap()).is_err());
    assert!(inventory.target_id_v1(m::FamilyMaterialTargetV1::Input(T::FullIds)).is_err());
    assert_eq!(before, state.state_root(), "inventory/authentication itself writes no completion or credit");
}

#[test]
fn family_material_native_semantic_tokenizer_and_prefix_inventories_are_complete_and_immutable() {
    use crate::palw_court_family_material_v1 as m;
    use crate::palw_court_family_supplier_v1::FamilySupplierRoleV1 as Role;
    use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1 as Source;
    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    let (mut state, _, _) = world(&fx, &fx.frame);
    state.class_semantics.insert(semantics.class_id, semantics.clone());
    state.claim_semantics.insert(fx.frame.plan_v1().original_claim, semantics.clone());
    state.staged_courts.insert(Key::Inventory(eof.artifact_hash), Row::ArtifactInventory(eof.clone()));
    let source = Source::new_native_v1(
        &fx.record,
        &fx.layout,
        fx.frame.header_v1(),
        fx.frame.original_v1().execution_root,
        fx.frame.original_v1().trace_root,
        fx.rules.maximum_leaves,
    )
    .unwrap();
    let profile = h(0xF42);
    let (graph, _numeric, cover, _) = aggregate_actual_geometry_fixture_v1(&fx, &source, profile);
    let program = crate::palw_native_resource_v1::decode_native_tir_program_v1(&fx.record.program).unwrap();
    assert!(m::derive_family_inventory_v1(&state,&fx.frame,&source,&graph,profile,&program,&cover,Some(&semantics),Role::SemanticTable).is_err(),
        "a schema cap is not an authenticated table EOF inventory");
    let table_bytes=crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&table);
    let checked_table=m::prepare_family_table_inventory_v1(&table_bytes,&semantics,source.vocab_v1()).unwrap();
    let semantic = m::derive_family_inventory_with_table_v1(
        &state,
        &fx.frame,
        &source,
        &graph,
        profile,
        &program,
        &cover,
        Some(&semantics),
        Role::SemanticTable,
        Some(&checked_table),
    )
    .unwrap();
    assert_eq!(
        semantic.targets_v1(),
        &[(m::FamilyMaterialTargetV1::SemanticTable, table_bytes.len() as u64)]
    );
    assert!(table.vocab() > 0);
    assert!(
        crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&table).len() as u64 <= semantic.max_material_bytes_v1()
    );
    let tokenizer = m::derive_family_inventory_v1(
        &state,
        &fx.frame,
        &source,
        &graph,
        profile,
        &program,
        &cover,
        Some(&semantics),
        Role::TokenizerArtifact,
    )
    .unwrap();
    assert_eq!(tokenizer.max_material_bytes_v1(), raw.len() as u64 + 4);
    assert_ne!(semantic.id_v1(), tokenizer.id_v1());
    state.staged_courts.remove(&Key::Inventory(eof.artifact_hash));
    assert!(
        m::derive_family_inventory_v1(
            &state,
            &fx.frame,
            &source,
            &graph,
            profile,
            &program,
            &cover,
            Some(&semantics),
            Role::TokenizerArtifact
        )
        .is_err(),
        "metadata alone does not stand for actual authenticated EOF"
    );
    let prefix_fx = crate::palw_tir_public_claim_v1::palw_tir_public_prefix_history_frame_fixture_v1(false);
    let (prefix_state, claim, _) = prefix_material_world(&prefix_fx);
    let schedule = material_test_schedule();
    let inventory = m::derive_current_family_inventory_v1(&prefix_state, claim, Role::PrefixGeneratedSelectionAndStop).unwrap();
    assert!(inventory.targets_v1().iter().any(|(target, _)| matches!(
        target,
        m::FamilyMaterialTargetV1::Input(crate::palw_court_shared_material_intake_v1::MaterialTargetV1::FullIds)
    )));
    assert!(inventory.targets_v1().iter().any(|(target, _)| matches!(
        target,
        m::FamilyMaterialTargetV1::Input(crate::palw_court_shared_material_intake_v1::MaterialTargetV1::Unit { .. })
    )));
    assert!(inventory.packet_count_v1().unwrap() >= 2 * inventory.targets_v1().len() as u64);
    let _ = schedule;
}

#[test]
fn family_material_native_transport_authenticates_completion_reuses_data_and_rolls_back_bad_finish() {
    use super::super::palw_court_family_material_fold_v1::apply_family_material_v1 as apply;
    use crate::palw_court_family_material_v1 as m;
    use crate::palw_court_family_supplier_v1::{
        FAMILY_SUPPLIER_TABLE_V1, FamilySupplierFeeStateV1, FamilySupplierRoleV1 as Role, FamilySupplierRowV1, FamilySupplierTargetV1,
        FamilySupplierTermsV1, sign_family_supplier_v1,
    };
    // This fixture starts from an accepted original and a signed admitted-contract
    // row. The independent native funding/EDF/fee lifecycle has its own regression.
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_material_frame_fixture_v1(true);
    let frame = &fx.frame;
    let mut schedule = material_test_schedule();
    schedule.sompi_per_byte = 1;
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(Some(schedule));
    let (mut state, params, mut extras, claim, _) = material_intake_world(&fx);
    let params = params.with_family_supplier_from(Some(0));
    extras.family_supplier_active = true;
    let network = extras.tir_public_network_domain_v1.unwrap();
    let ruleset = h(0x42F0);
    let who = outsider();
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([117; 32]);
    state.bonds.get_mut(&who).unwrap().pubkey = key.verification_key.as_ref().to_vec();
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new_typed(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(network, ruleset),
        None,
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: frame.plan_v1().terms.accepted_daa, burned: 0 },
    ));
    let inventory = m::derive_current_family_inventory_v1(&state, claim, Role::ParameterAndPrompt).unwrap();
    let hard = crate::palw_court_shared_tir_state_v1::context(&state, claim).unwrap().hard_daa;
    let mut terms = FamilySupplierTermsV1 {
        version: 1,
        network,
        ruleset,
        claim,
        source: inventory.source_v1(),
        relative_graph: inventory.relative_graph_v1(),
        role: Role::ParameterAndPrompt,
        inventory: inventory.id_v1(),
        supplier: who,
        sponsor: who,
        max_material_bytes: inventory.max_material_bytes_v1(),
        accept_from_daa: frame.plan_v1().terms.accepted_daa,
        accept_until_daa: frame.plan_v1().terms.accepted_daa + 1,
        respond_until_daa: hard,
        material_until_daa: frame.plan_v1().terms.material_until_daa,
        fee_profile: h(1),
        capital_profile: h(2),
        retention_profile: h(3),
        settlement_policy: h(4),
        fee_sompi: 1,
        collateral_sompi: 1,
        nonce: h(0x42F1),
    };
    terms = super::super::palw_court_family_supplier_fold_v1::palw_family_supplier_terms_quote_v1(&inventory, terms).unwrap();
    let consent = sign_family_supplier_v1(terms, |_, message, context| {
        Some(libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [118; 32]).unwrap().as_ref().to_vec())
    })
    .unwrap();
    let contract = consent.id_v1().unwrap();
    let mut targets = inventory
        .targets_v1()
        .iter()
        .map(|(target, n)| FamilySupplierTargetV1 { id: inventory.target_id_v1(*target).unwrap(), max_bytes: *n, packets: vec![[0x42;64]] })
        .collect::<Vec<_>>();
    targets.sort_by_key(|t| t.id);
    let row = FamilySupplierRowV1 {
        version: 2,
        consent,
        allocation: frame.plan_v1().allocation_id,
        accepted_daa: frame.plan_v1().terms.accepted_daa,
        targets,
        completed: Default::default(),
        defaulted_daa: None,
        fee_state: FamilySupplierFeeStateV1::Held,
    };
    state.set_kernel_aux_row_for_tests(
        FAMILY_SUPPLIER_TABLE_V1,
        contract.as_byte_slice().to_vec(),
        Some(borsh::to_vec(&row).unwrap()),
    );
    let status=crate::palw_court_family_supplier_v1::FamilySupplierStatusV2 {
        version:2,contract,defaulted_daa:None,fee_state:FamilySupplierFeeStateV1::Held,
    };
    state.set_kernel_aux_row_for_tests(FAMILY_SUPPLIER_TABLE_V1,
        crate::palw_court_family_supplier_v1::family_status_key_v2(contract).to_vec(),Some(borsh::to_vec(&status).unwrap()));
    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let target = m::FamilyMaterialTargetV1::Input(crate::palw_court_shared_material_intake_v1::MaterialTargetV1::PromptFlat);
    let body = borsh::to_vec(&fx.execution.prompt).unwrap();
    let digest = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
    let draft = m::family_material_draft_id_v1(contract, target, who, body.len() as u64, digest).unwrap();
    let signed = |action| {
        m::sign_family_material_v1(
            m::SignedFamilyMaterialV1 { version: 1, contract, target, submitter: who, action, signature: vec![] },
            network,
            |_, message, context| {
                Some(libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [119; 32]).unwrap().as_ref().to_vec())
            },
        )
        .unwrap()
    };
    let start = frame.plan_v1().terms.accepted_daa + 2;
    let begin = signed(m::FamilyMaterialActionV1::Begin { total_bytes: body.len() as u64, body_hash: digest });
    let mut blocked_state = state.clone();
    blocked_state.reserved_exposure.insert(who, u128::from(blocked_state.bond(&who).unwrap().collateral));
    let mut blocked = TransitionBuilder::new(&blocked_state, &params, false, false, false, false, &extras);
    let blocked_root = blocked.state.state_root();
    assert!(apply(&mut blocked, &at(frame, start), &begin).is_err());
    assert_eq!(blocked_root, blocked.state.state_root(), "total collateral cannot fund storage by consuming held principal");
    // An existing indexed Court session is a separate A-6 deposit after
    // R-core; preserving A-1 alone would let this signed Begin burn its funds.
    let rcore = params.clone().with_rcore_plus_mirrors(Some(0), 0, Vec::new());
    let mut court_parent = TransitionBuilder::new(&state, &rcore, false, false, false, false, &extras);
    let ladder = crate::palw_bisect::PalwBisectLadderV1::open(
        &h(0x4241), &h(0x4242), &h(0x4243), &h(0x4244),
        crate::palw_bisect::PalwBisectSpaceV1::StepLeaves, 8, start, start + 24,
    ).unwrap();
    court_parent.write_court(h(0x4245), Some(PalwCourtSessionStateV2 {
        claim, challenger_bond: who, opened_daa: start,
        deadline_daa: start + rcore.window_court(), ladder, dissection: None,
    })).unwrap();
    let accuser = court_parent.read().accuser_ledger_v1(&who, start);
    assert!(accuser > 0, "real indexed Court session reserves its actual claim deposit");
    let a1 = court_parent.committed_at(&who, start).unwrap();
    for free in [body.len() as u64 - 1, body.len() as u64] {
        let parent = court_parent.state.clone();
        let mut attempted = TransitionBuilder::new(&parent, &rcore, false, false, false, false, &extras);
        let keep = u64::try_from(a1 + accuser + u128::from(free)).unwrap();
        assert!(keep >= rcore.min_collateral_sompi());
        let surplus = attempted.state.bond(&who).unwrap().collateral.checked_sub(keep).unwrap();
        attempted.slash_bond(who, u128::from(surplus)).unwrap();
        assert_eq!(super::super::palw_court_family_supplier_fold_v1::family_free_principal_v1(&attempted, &who, start).unwrap(), u128::from(free));
        let saved = attempted.state.state_root();
        let journal = borsh::to_vec(&attempted.entries).unwrap();
        let result = apply(&mut attempted, &at(frame, start), &begin);
        if free < body.len() as u64 {
            assert!(result.unwrap_err().to_string().contains("uncommitted principal"));
            assert_eq!(attempted.state.state_root(), saved);
            assert_eq!(borsh::to_vec(&attempted.entries).unwrap(), journal);
        } else {
            result.unwrap();
            assert_eq!(u128::from(attempted.state.bond(&who).unwrap().collateral), a1 + accuser);
            assert_eq!(attempted.read().accuser_ledger_v1(&who, start), accuser);
        }
    }
    apply(&mut b, &at(frame, start), &begin).unwrap();
    let before = b.state.state_root();
    assert!(apply(&mut b, &at(frame, start + 1), &signed(m::FamilyMaterialActionV1::Finish { draft })).is_err());
    assert_eq!(before, b.state.state_root());
    let part = signed(m::FamilyMaterialActionV1::Part { draft, offset: 0, bytes: body.clone() });
    apply(&mut b, &at(frame, start + 1), &part).unwrap();
    let root = b.state.state_root();
    apply(&mut b, &at(frame, start + 1), &part).unwrap();
    assert_eq!(root, b.state.state_root(), "exact chunk retry is idempotent");
    let mut altered = body.clone();
    altered[4] ^= 1;
    assert!(
        apply(&mut b, &at(frame, start + 1), &signed(m::FamilyMaterialActionV1::Part { draft, offset: 0, bytes: altered })).is_err()
    );
    assert_eq!(root, b.state.state_root());
    apply(&mut b, &at(frame, start + 2), &signed(m::FamilyMaterialActionV1::Finish { draft })).unwrap();
    let completed =
        super::super::palw_court_family_supplier_fold_v1::family_contract_for_material_v1(&b.state, contract).unwrap().unwrap();
    assert_eq!(completed.completed.get(&inventory.target_id_v1(target).unwrap()), Some(&digest));
    assert!(matches!(completed.fee_state, FamilySupplierFeeStateV1::Held), "supply does not accelerate the fixed fee horizon");
    let root = b.state.state_root();
    apply(&mut b, &at(frame, start + 3), &begin).unwrap();
    assert_eq!(root, b.state.state_root(), "canonical authenticated reuse creates no second draft or fee");
    // Replaying the actual journal backwards restores the exact accepted-contract
    // baseline, including both immutable chunks and authenticated material rows.
    let mut undone = b.state.clone();
    for entry in b.entries.iter().rev() {
        apply_delta_entry(&mut undone, entry, true).unwrap();
    }
    assert_eq!(undone.state_root(), state.state_root());
}

#[test]
fn family_material_four_role_prefix_corpus_binds_real_tokenizer_before_class_and_execution() {
    use crate::palw_court_family_material_v1 as m;
    use crate::palw_court_family_supplier_v1::FamilySupplierRoleV1 as Role;
    use crate::palw_court_shared_material_intake_v1::MaterialTargetV1 as Target;
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    let key=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x6D;32]);
    let (fx,sem,eof,raw,table)=crate::palw_tir_public_claim_v1::palw_tir_public_prefix_tokenizer_frame_with_executor_fixture_v1(key.verification_key.as_ref());
    let job=&fx.frame.original_v1().job;
    assert_eq!(job.version,crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION);
    assert_eq!(job.executor_pubkey,key.verification_key.as_ref());
    assert_eq!(job.tokenizer_id,crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw));
    assert_eq!(fx.execution.binding.class.tokenizer_id,job.tokenizer_id);
    assert_eq!(fx.execution.binding.class.class_id(&fx.execution.binding.artifact_root),job.class_id);
    assert_eq!(crate::palw_fp_execution_v3::palw_fp_job_pin_v1(fx.frame.original_v1()),
        crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&fx.execution.binding.job_context));
    assert_eq!(eof.actual_bytes,raw.len() as u64);
    material_test_schedule();
    let (mut state,_,_,claim,_)=material_intake_world(&fx);
    state.class_semantics.insert(sem.class_id,sem.clone());
    state.claim_semantics.insert(claim,sem);
    state.staged_courts.insert(Key::Inventory(eof.artifact_hash),Row::ArtifactInventory(eof));
    state.kernel_route=Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new_typed(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(0x4E7),h(0x42)),None,None,
        misaka_palw_kernel::rows::LedgerScalarsV1{daa:100,burned:0}));
    let (_,params,extras)=world(&fx,&fx.frame);
    let mut builder=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
    super::super::palw_court_family_material_fold_v1::install_family_table_inventory_v1(&mut builder,claim,&crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&table),crate::tx::TransactionOutpoint::new(h(0xF42),0)).unwrap();
    state=builder.state.clone();
    let mut inventories=std::collections::BTreeSet::new();
    for role in [Role::ParameterAndPrompt,Role::SemanticTable,Role::TokenizerArtifact,Role::PrefixGeneratedSelectionAndStop] {
        let inventory=m::derive_current_family_inventory_v1(&state,claim,role).unwrap();
        assert!(inventories.insert(inventory.id_v1()));
        assert!(!inventory.targets_v1().is_empty());
        assert!(inventory.packet_count_v1().unwrap()>=2*inventory.targets_v1().len() as u64);
        for &(target,_) in inventory.targets_v1() {
            let body=match target {
                m::FamilyMaterialTargetV1::SemanticTable=>crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&table),
                m::FamilyMaterialTargetV1::TokenizerRaw=>borsh::to_vec(&raw).unwrap(),
                m::FamilyMaterialTargetV1::Input(Target::PromptFlat)=>borsh::to_vec(&fx.execution.prompt).unwrap(),
                m::FamilyMaterialTargetV1::Input(Target::Parameter{leaf})=>borsh::to_vec(&fx.execution.param_opening(leaf).unwrap()).unwrap(),
                // Other native source units require their actual Merkle openings;
                // complete all-branch inventory is still derived, never declared.
                _=>continue,
            };
            m::authenticate_family_material_v1(&state,&inventory,target,&body).unwrap();
            let mut wrong=body;*wrong.last_mut().unwrap()^=1;
            assert!(m::authenticate_family_material_v1(&state,&inventory,target,&wrong).is_err());
        }
        if role==Role::PrefixGeneratedSelectionAndStop {
            assert!(inventory.targets_v1().iter().any(|(target,_)|*target==m::FamilyMaterialTargetV1::Input(Target::FullIds)));
            assert!(inventory.targets_v1().iter().any(|(target,_)|matches!(target,m::FamilyMaterialTargetV1::Input(Target::Unit{..}))));
        }
    }
    assert_eq!(inventories.len(),4);
}

// Exact ordinary signed Begin/Part/Finish writes. The original/contract fixture
// is explicitly pre-admitted, as in the existing transport test; no credit or
// mainnet/genesis approval is inferred from it.
#[test]
fn rfc23s2_owner_local_signed_family_transport_matches_full_expected_cold_and_reorg(){
    use super::super::palw_court_family_material_fold_v1::apply_family_material_v1 as apply;
    use crate::palw_court_family_material_v1 as m;
    use crate::palw_court_family_supplier_v1::{
        FAMILY_SUPPLIER_TABLE_V1, FamilySupplierFeeStateV1, FamilySupplierRoleV1 as Role, FamilySupplierRowV1, FamilySupplierTargetV1,
        FamilySupplierTermsV1, sign_family_supplier_v1,
    };
    // This fixture starts from an accepted original and a signed admitted-contract
    // row. The independent native funding/EDF/fee lifecycle has its own regression.
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_material_frame_fixture_v1(true);
    let frame = &fx.frame;
    let mut schedule = material_test_schedule();
    schedule.sompi_per_byte = 1;
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(Some(schedule));
    let (mut state, params, mut extras, claim, _) = material_intake_world(&fx);
    let params = params.with_family_supplier_from(Some(0));
    extras.family_supplier_active = true;
    let network = extras.tir_public_network_domain_v1.unwrap();
    let ruleset = h(0x42F0);
    let who = outsider();
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([117; 32]);
    state.bonds.get_mut(&who).unwrap().pubkey = key.verification_key.as_ref().to_vec();
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new_typed(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(network, ruleset),
        None,
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: frame.plan_v1().terms.accepted_daa, burned: 0 },
    ));
    let inventory = m::derive_current_family_inventory_v1(&state, claim, Role::ParameterAndPrompt).unwrap();
    let hard = crate::palw_court_shared_tir_state_v1::context(&state, claim).unwrap().hard_daa;
    let mut terms = FamilySupplierTermsV1 {
        version: 1,
        network,
        ruleset,
        claim,
        source: inventory.source_v1(),
        relative_graph: inventory.relative_graph_v1(),
        role: Role::ParameterAndPrompt,
        inventory: inventory.id_v1(),
        supplier: who,
        sponsor: who,
        max_material_bytes: inventory.max_material_bytes_v1(),
        accept_from_daa: frame.plan_v1().terms.accepted_daa,
        accept_until_daa: frame.plan_v1().terms.accepted_daa + 1,
        respond_until_daa: hard,
        material_until_daa: frame.plan_v1().terms.material_until_daa,
        fee_profile: h(1),
        capital_profile: h(2),
        retention_profile: h(3),
        settlement_policy: h(4),
        fee_sompi: 1,
        collateral_sompi: 1,
        nonce: h(0x42F1),
    };
    terms = super::super::palw_court_family_supplier_fold_v1::palw_family_supplier_terms_quote_v1(&inventory, terms).unwrap();
    let consent = sign_family_supplier_v1(terms, |_, message, context| {
        Some(libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [118; 32]).unwrap().as_ref().to_vec())
    })
    .unwrap();
    let contract = consent.id_v1().unwrap();
    let mut targets = inventory
        .targets_v1()
        .iter()
        .map(|(target, n)| FamilySupplierTargetV1 { id: inventory.target_id_v1(*target).unwrap(), max_bytes: *n, packets: vec![[0x42;64]] })
        .collect::<Vec<_>>();
    targets.sort_by_key(|t| t.id);
    let row = FamilySupplierRowV1 {
        version: 2,
        consent,
        allocation: frame.plan_v1().allocation_id,
        accepted_daa: frame.plan_v1().terms.accepted_daa,
        targets,
        completed: Default::default(),
        defaulted_daa: None,
        fee_state: FamilySupplierFeeStateV1::Held,
    };
    state.set_kernel_aux_row_for_tests(
        FAMILY_SUPPLIER_TABLE_V1,
        contract.as_byte_slice().to_vec(),
        Some(borsh::to_vec(&row).unwrap()),
    );
    let status=crate::palw_court_family_supplier_v1::FamilySupplierStatusV2 {
        version:2,contract,defaulted_daa:None,fee_state:FamilySupplierFeeStateV1::Held,
    };
    state.set_kernel_aux_row_for_tests(FAMILY_SUPPLIER_TABLE_V1,
        crate::palw_court_family_supplier_v1::family_status_key_v2(contract).to_vec(),Some(borsh::to_vec(&status).unwrap()));
    let target = m::FamilyMaterialTargetV1::Input(crate::palw_court_shared_material_intake_v1::MaterialTargetV1::PromptFlat);
    let body = borsh::to_vec(&fx.execution.prompt).unwrap();
    let digest = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
    let draft = m::family_material_draft_id_v1(contract, target, who, body.len() as u64, digest).unwrap();
    let signed = |action| {
        m::sign_family_material_v1(
            m::SignedFamilyMaterialV1 { version: 1, contract, target, submitter: who, action, signature: vec![] },
            network,
            |_, message, context| {
                Some(libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [119; 32]).unwrap().as_ref().to_vec())
            },
        )
        .unwrap()
    };
    let start = frame.plan_v1().terms.accepted_daa + 2;
    let begin = signed(m::FamilyMaterialActionV1::Begin { total_bytes: body.len() as u64, body_hash: digest });

    use super::super::{palw_claim_local_checkpoint_fold_v1 as fold,palw_claim_local_checkpoint_descriptor_v3 as d3,palw_claim_local_context_store_v2 as store};
    let params=params.with_claim_local_settlement_from_daa(Some(0));
    let p0=at(frame,start-1);let(cp,bases)=fold::prepared_expected_for_test_v3(&state,&params,&p0).unwrap();
    let mut seed=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);d3::write_prepared_v3(&mut seed,&cp,&bases).unwrap();
    let old=seed.state.last_point;seed.state.last_point=Some(p0);seed.entries.push(PalwDeltaEntryV2::LastPoint{old,new:Some(p0)});
    let mut parent=seed.state;
    let actions=[begin,signed(m::FamilyMaterialActionV1::Part{draft,offset:0,bytes:body.clone()}),signed(m::FamilyMaterialActionV1::Finish{draft})];
    for (offset,control) in actions.into_iter().enumerate(){
        let point=at(frame,start+offset as u64);let mut b=TransitionBuilder::new(&parent,&params,false,false,false,false,&extras);
        let previous=parent.state_root();let old_roots=d3::record_roots_for_test_v3(&parent,crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1::ChainClaim{claim}).unwrap();
        // Match the actual full-fold boundary: all owner publications finish
        // before physical GC. The old collector below is a test-only oracle.
        b.claim_local_gc_defer_v3=true;
        apply(&mut b,&point,&control).unwrap();let object=PalwConsensusObjectV2::CourtFamilyMaterialV1{control:Box::new(control)};
        let frontier_bytes=borsh::to_vec(&(b.state.safe_frontier_blue_score,b.state.safe_frontier)).unwrap();
        let frontier_entries:Vec<_>=b.entries.iter().filter_map(|e|match e{PalwDeltaEntryV2::Frontier{old,new}=>Some((*old,*new)),_=>None}).collect();
        assert!(d3::try_material_checkpoint_v3(&mut b,&parent,&point,&[object],PalwBlockWorkV3::None,&[]).unwrap(),"normal authenticated material action must use actual local path");
        assert_eq!(borsh::to_vec(&(b.state.safe_frontier_blue_score,b.state.safe_frontier)).unwrap(),frontier_bytes,"local path leaves frontier bytes unchanged");
        assert_eq!(b.entries.iter().filter_map(|e|match e{PalwDeltaEntryV2::Frontier{old,new}=>Some((*old,*new)),_=>None}).collect::<Vec<_>>(),frontier_entries,"local path leaves frontier entries unchanged");
        let before_free=b.state.clone();let prefix=b.entries.clone();
        let mut oracle=TransitionBuilder::new(&before_free,&params,false,false,false,false,&extras);
        store::collect_unreachable_reference_v3(&mut oracle).unwrap();
        super::super::palw_claim_local_context_gc_v3::settle_state_v3(&mut oracle.state).unwrap();
        super::super::palw_claim_local_context_gc_v3::finish_fold_v3(&mut b).unwrap();
        assert_eq!(b.state.kernel_route().unwrap().aux,oracle.state.kernel_route().unwrap().aux,"signed material local path has identical live N/B/I/A closure to complete mark/sweep");
        assert_eq!(b.state.claim_local_gc_v3,oracle.state.claim_local_gc_v3);
        let old=b.state.last_point;b.state.last_point=Some(point);b.entries.push(PalwDeltaEntryV2::LastPoint{old,new:Some(point)});
        let mut legacy_entries=prefix;legacy_entries.extend(oracle.entries.clone());legacy_entries.push(PalwDeltaEntryV2::LastPoint{old,new:Some(point)});
        let legacy_delta=PalwStateDeltaV2{point,entries:legacy_entries};
        let legacy_replayed=super::super::apply_delta_v2(&parent,&legacy_delta,&params).unwrap();
        assert_eq!(legacy_replayed.state_root(),b.state.state_root());assert_eq!(legacy_replayed.claim_local_gc_v3,b.state.claim_local_gc_v3);
        let legacy_reverted=super::super::revert_delta_v2(&legacy_replayed,&legacy_delta,&params).unwrap();
        assert_eq!(legacy_reverted.state_root(),parent.state_root());assert_eq!(legacy_reverted.claim_local_gc_v3,parent.claim_local_gc_v3,"real B-before-N replay/reorg returns exact final parent cache");
        let delta=PalwStateDeltaV2{point,entries:b.entries.clone()};let replayed=super::super::apply_delta_v2(&parent,&delta,&params).unwrap();
        assert_eq!(replayed.state_root(),b.state.state_root());assert_eq!(replayed.claim_local_gc_v3,b.state.claim_local_gc_v3);
        let reverted=super::super::revert_delta_v2(&replayed,&delta,&params).unwrap();assert_eq!(reverted.state_root(),parent.state_root());assert_eq!(reverted.claim_local_gc_v3,parent.claim_local_gc_v3);

        let expected=fold::expected_claim_local_checkpoint_v1(&b.state,&params,&point).unwrap().unwrap();let stored=d3::load(&b.state).unwrap().unwrap();assert_eq!(*stored,*expected);
        let new_roots=d3::record_roots_for_test_v3(&b.state,crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1::ChainClaim{claim}).unwrap();assert_eq!(old_roots[0],new_roots[0]);assert_eq!(old_roots[1],new_roots[1]);
        d3::validate(&b.state).unwrap();store::validate_context_rows_v2(&b.state,&params).unwrap();
        let wire=borsh::to_vec(b.state.kernel_route().unwrap()).unwrap();let mut cold=b.state.clone();cold.kernel_route=Some(borsh::from_slice(&wire).unwrap());fold::assert_claim_local_checkpoint_v1(&cold,&params).unwrap();assert_eq!(*d3::load(&cold).unwrap().unwrap(),*expected);
        let mut rebuilt=cold.clone();super::super::palw_claim_local_context_gc_v3::rebuild_cold_v3(&mut rebuilt).unwrap();assert_eq!(rebuilt.claim_local_gc_v3,b.state.claim_local_gc_v3);

        let mut undone=b.state.clone();for entry in b.entries.iter().rev(){super::super::apply_delta_entry(&mut undone,entry,true).unwrap();}assert_eq!(undone.state_root(),previous);assert_eq!(*d3::load(&undone).unwrap().unwrap(),*d3::load(&parent).unwrap().unwrap());
        parent=b.state;
    }
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(None);
}
