// Actual program + native allocation/prepare + exact EOF primitive certificate.
// Original pledge setup is the same explicit verified-half fixture, not mined
// original admission. Public raw/proof delivery and paid installation are absent.
#[test]
fn shared_tokenizer_builder_uses_native_original_eof_and_rechecks_authenticated_material() {
    use crate::palw_court_shared_tir_builder_store_v1::{
        tokenizer_terminal::{
            SignedTokenizerTerminalIntentV1 as Intent, TokenizerMaterialV1 as Material,
            TOKENIZER_INTENT_SIGNATURE_CONTEXT_V1 as SIGNATURE_CONTEXT,
        },
        SharedTirBuilderStoreV1 as Store,
    };
    use crate::palw_court_shared_tir_state_v1 as shared;
    use crate::palw_tokenizer_table_stream_v1::canonical_replay::Budget;
    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    fx.record.check_program_v1().unwrap();
    assert!(fx.record.program.len() > 3);
    let frame = &fx.frame;
    let p = frame.plan_v1();
    let claim = p.original_claim;
    assert!(p
        .semantic
        .as_ref()
        .unwrap()
        .stages
        .iter()
        .any(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Tokenizer));
    assert_eq!(fx.record.tokenizer_id, crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw));
    let (mut state, params, mut extras) = world(&fx, frame);
    state.class_semantics.insert(semantics.class_id, semantics.clone());
    state.claim_semantics.insert(claim, semantics.clone());
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([93; 32]);
    state.bonds.get_mut(&outsider()).unwrap().pubkey = key.verification_key.as_ref().to_vec();
    let producer = frame.custody_v1().pledge_v1().producer;
    let producer_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([94; 32]);
    state.bonds.get_mut(&producer).unwrap().pubkey = producer_key.verification_key.as_ref().to_vec();
    let mut pledge = crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1 {
        version: 1,
        claim,
        producer,
        execution_root: frame.original_v1().execution_root,
        job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(frame.original_v1()),
        cu_ruleset_id: semantics.cu_ruleset_id,
        resource_plan_id: p.semantic_resource_plan_id.unwrap(),
        maximum_records: p.maximum_semantic_history_records,
        maximum_custodians: p.acquisition.maximum_custodians,
        accept_until_daa: p.terms.accepted_daa + 1,
        material_until_daa: p.terms.material_until_daa,
        signature: vec![],
    };
    pledge.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &producer_key.signing_key,
        pledge.pledge_id_v1().as_byte_slice(),
        crate::palw_history_custody_v1::PALW_HISTORY_PLEDGE_CONTEXT_V1,
        [95; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    extras.staged_public_court = Some(fx.rules.config);
    extras.history_custody_pledges.insert(claim, pledge);
    let mut dependency = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    dependency.write_staged_row_v1(Key::Inventory(eof.artifact_hash), Some(Row::ArtifactInventory(eof.clone()))).unwrap();
    let bundle = frame_semantic_bundle_v1(&dependency, frame, p.terms.accepted_daa).unwrap().unwrap();
    dependency.write_staged_row_v1(Key::Claim(claim), Some(bundle)).unwrap();
    let state = bind_clock_service(&dependency.state, frame, &params, &extras);
    let source = frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
    let profile = shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
    let scope = set.scope_id().unwrap();
    let c = shared::SharedTirContextV1 {
        version: 1,
        claim,
        allocation_id: p.allocation_id,
        source_id: p.source_id,
        rules_id: p.rules_id,
        profile_id: profile,
        scope_id: scope,
        queries: set.len(),
        original_accepted_daa: p.terms.accepted_daa,
        hard_daa: p.terms.accepted_daa + 10_000,
        material_until_daa: p.terms.material_until_daa,
    };
    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    b.write_staged_row_v1(Key::CourtSharedContext(claim), Some(Row::CourtSharedContext(Box::new(c)))).unwrap();
    let store = Store::new(&b.state, claim, scope, &[], &[], None).unwrap();
    // Existing registered program, signed custody/semantic bundle and rooted EOF:
    // private retained all-ID data authentication, not funded original admission.
    {
        let encoded=(0..table.vocab()).map(|id|borsh::to_vec(&table.opening_of(id).unwrap()).unwrap()).collect::<Vec<_>>();
        let bodies=encoded.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let eog=borsh::to_vec(&table.eog_opening().unwrap()).unwrap();
        let owner=store.retain_semantic_material_v1(&b.state,claim,&bodies,&eog,&raw).unwrap();
        assert_eq!(owner.counts_v1().vocabulary,table.vocab());
        assert_eq!(owner.counts_v1().raw_bytes,eof.actual_bytes);
        assert_eq!(owner.raw_v1(),raw);
        owner.check_scope_v1(p.source_id,claim,scope,semantics.commitment(),eof.inventory_id).unwrap();
        assert!(owner.check_scope_v1(h(996),claim,scope,semantics.commitment(),eof.inventory_id).is_err());
        assert!(store.retain_semantic_material_v1(&b.state,h(997),&bodies,&eog,&raw).is_err());
        assert!(store.retain_semantic_material_v1(&b.state,claim,&bodies[..bodies.len()-1],&eog,&raw).is_err());
        let mut changed=raw.clone();changed[0]^=1;
        assert!(store.retain_semantic_material_v1(&b.state,claim,&bodies,&eog,&changed).is_err());
    }
    // Production data paths generated under a local target context; the final signed
    // material intent below uses a different context and rechecks the same paths.
    let rendering = table.opening_of(0).unwrap();
    let view=crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(source.header_v1(),&fx.record,frame.descriptor_v1().maximum_leaves).unwrap();
    let target=crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state,&claim).unwrap();
    let base=crate::palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1::new_with_operand_length_view_v1(
        h(961),&target,&view,&frame.original_v1().job,0,rendering.rendering.is_some(),
        rendering.rendering.as_ref().map_or(0,Vec::len) as u32,&rendering.opening,eof.actual_bytes).unwrap();
    let produced=crate::palw_tokenizer_table_stream_v1::producer::generate(&base,&raw,
        crate::palw_tokenizer_table_stream_v1::producer::Budget{max_proofs:1000,max_nodes:513000,max_requests:1<<30,max_hash_calls:1000000,max_hash_bytes:1<<30}).unwrap();
    let proofs=produced.proofs();
    assert!(!proofs.is_empty());
    let material = Material { raw: &raw, proofs, rendering: &rendering };
    let budget = Budget { max_material_bytes: 1 << 25, max_proofs: 1000, max_native_calls: 100_000, max_packets: 10_000 };
    let rules = crate::palw_court_close_receipt_v1::NativeCourtRulesV1 {
        network_domain: extras.tir_public_network_domain_v1.unwrap(),
        at: at(frame, 1),
        court: fx.rules.court,
        step_ladder: fx.rules.maximum_leaves,
        prompt_ids_merkle: false,
        held_regime: false,
        decode_close_convicts_only: false,
        fp_decode_from_daa: None,
        cell_closure_from_daa: None,
        memory_routing_from_daa: None,
        tir_active: true,
        gen_active: false,
        task_heads_active: false,
        improvement_active: false,
        constraint_active: false,
    };
    let mut intent = Intent {
        version: 1,
        claim,
        scope_id: scope,
        policy_id: crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&rules).unwrap(),
        semantics_id: semantics.commitment(),
        inventory_id: eof.inventory_id,
        material_id: material.identity(budget).unwrap(),
        token_id: 0,
        filer: outsider(),
        signature: vec![],
    };
    let sign = |i: &mut Intent| {
        i.signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            i.signing_message(rules.network_domain).unwrap().as_byte_slice(),
            SIGNATURE_CONTEXT,
            [96; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
    };
    sign(&mut intent);
    let before = b.state.state_root();
    let evidence = store.build_tokenizer_terminal(&b.state, &params, &rules, &intent, material, budget).unwrap();
    // Borrow the exact retained decoded payload owners: no raw/rendering clone
    // between native material owner and the real existing terminal primitive.
    let all_encoded=(0..table.vocab()).map(|id|borsh::to_vec(&table.opening_of(id).unwrap()).unwrap()).collect::<Vec<_>>();
    let all_bodies=all_encoded.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let eog_encoded=borsh::to_vec(&table.eog_opening().unwrap()).unwrap();
    let retained=store.retain_semantic_material_v1(&b.state,claim,&all_bodies,&eog_encoded,&raw).unwrap();
    let retained_evidence=store.build_tokenizer_terminal_retained_v1(&b.state,&params,&rules,&intent,&retained,proofs,budget).unwrap();
    assert_eq!(retained_evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap(),
        evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap());
    let mut wrong=intent.clone();wrong.inventory_id=h(966);sign(&mut wrong);
    assert!(store.build_tokenizer_terminal_retained_v1(&b.state,&params,&rules,&wrong,&retained,proofs,budget).is_err());
    let mut wrong=intent.clone();wrong.scope_id=h(967);sign(&mut wrong);
    assert!(store.build_tokenizer_terminal_retained_v1(&b.state,&params,&rules,&wrong,&retained,proofs,budget).is_err());
    let mut wrong=intent.clone();wrong.token_id=table.vocab();sign(&mut wrong);
    assert!(store.build_tokenizer_terminal_retained_v1(&b.state,&params,&rules,&wrong,&retained,proofs,budget).is_err());
    assert!(store.build_tokenizer_terminal_retained_v1(&b.state,&params,&rules,&intent,&retained,&proofs[..proofs.len()-1],budget).is_err());
    assert!(!evidence.recheck(&store, &b.state, &params, &rules, budget).unwrap());
    let mut changed = intent.clone();
    changed.scope_id = h(962);
    sign(&mut changed);
    assert!(store.build_tokenizer_terminal(&b.state, &params, &rules, &changed, material, budget).is_err());
    let mut changed = intent.clone();
    changed.inventory_id = h(963);
    sign(&mut changed);
    assert!(store.build_tokenizer_terminal(&b.state, &params, &rules, &changed, material, budget).is_err());
    let missing = Material { proofs: &proofs[..proofs.len() - 1], ..material };
    let mut changed = intent.clone();
    changed.material_id = missing.identity(budget).unwrap();
    sign(&mut changed);
    assert!(store.build_tokenizer_terminal(&b.state, &params, &rules, &changed, missing, budget).is_err());
    // Deliberate malformed test operand, never used by the positive owner path.
    let mut foreign = proofs.to_vec();
    foreign[0].namespace ^= 1;
    let bad = Material { proofs: &foreign, ..material };
    let mut changed = intent.clone();
    changed.material_id = bad.identity(budget).unwrap();
    sign(&mut changed);
    assert!(store.build_tokenizer_terminal(&b.state, &params, &rules, &changed, bad, budget).is_err());
    let mut bad_rendering = rendering.clone();
    bad_rendering.rendering.as_mut().unwrap()[0] = b'X';
    let bad = Material { rendering: &bad_rendering, ..material };
    let mut changed = intent.clone();
    changed.material_id = bad.identity(budget).unwrap();
    sign(&mut changed);
    assert!(store.build_tokenizer_terminal(&b.state, &params, &rules, &changed, bad, budget).is_err());
    let mut changedraw = raw.clone();
    let a = changedraw.iter().position(|b| *b == b'A').unwrap();
    changedraw[a] = b'X';
    let bad = Material { raw: &changedraw, ..material };
    let mut changed = intent.clone();
    changed.material_id = bad.identity(budget).unwrap();
    sign(&mut changed);
    assert!(store.build_tokenizer_terminal(&b.state, &params, &rules, &changed, bad, budget).is_err());
    let mut no_eof = b.state.clone();
    no_eof.staged_courts.remove(&Key::Inventory(eof.artifact_hash));
    assert!(evidence.recheck(&store, &no_eof, &params, &rules, budget).is_err());
    for daa in [p.terms.accepted_daa - 1, p.terms.accepted_daa + 10_000] {
        let mut clock = rules;
        clock.at.daa_score = daa;
        assert!(evidence.recheck(&store, &b.state, &params, &clock, budget).is_err());
    }
    assert_eq!(b.state.state_root(), before);
}
