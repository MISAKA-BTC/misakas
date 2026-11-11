// Actual program + native allocation/prepare + exact EOF primitive certificate.
// Original pledge setup is the same explicit verified-half fixture, not mined
// original admission. Public raw/proof delivery and paid installation are absent.
#[test]
fn shared_tokenizer_builder_uses_native_original_eof_and_rechecks_authenticated_material() {
    use crate::palw_court_shared_tir_builder_store_v1::{
        SharedTirBuilderStoreV1 as Store,
        tokenizer_terminal::{
            SignedTokenizerTerminalIntentV1 as Intent, TOKENIZER_INTENT_SIGNATURE_CONTEXT_V1 as SIGNATURE_CONTEXT,
            TokenizerMaterialV1 as Material,
        },
    };
    use crate::palw_court_shared_tir_state_v1 as shared;
    use crate::palw_tokenizer_table_stream_v1::canonical_replay::Budget;
    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    fx.record.check_program_v1().unwrap();
    assert!(fx.record.program.len() > 3);
    let frame = &fx.frame;
    let p = frame.plan_v1();
    let claim = p.original_claim;
    assert!(
        p.semantic.as_ref().unwrap().stages.iter().any(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Tokenizer)
    );
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
    crate::palw_court_rooted_semantic_v1::install_material_for_consumer_test_v1(
        &mut b.state,
        claim,
        crate::palw_court_family_supplier_v1::FamilySupplierRoleV1::SemanticTable,
        crate::palw_court_family_material_v1::FamilyMaterialTargetV1::SemanticTable,
        crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&table),
    );
    crate::palw_court_rooted_semantic_v1::install_material_for_consumer_test_v1(
        &mut b.state,
        claim,
        crate::palw_court_family_supplier_v1::FamilySupplierRoleV1::TokenizerArtifact,
        crate::palw_court_family_material_v1::FamilyMaterialTargetV1::TokenizerRaw,
        borsh::to_vec(&raw).unwrap(),
    );
    let store = Store::new(&b.state, claim, scope, &[], &[], None).unwrap();
    // Existing registered program, signed custody/semantic bundle and rooted EOF:
    // private retained all-ID data authentication, not funded original admission.
    {
        let encoded = (0..table.vocab()).map(|id| borsh::to_vec(&table.opening_of(id).unwrap()).unwrap()).collect::<Vec<_>>();
        let bodies = encoded.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let eog = borsh::to_vec(&table.eog_opening().unwrap()).unwrap();
        let owner = store.retain_semantic_material_v1(&b.state, claim, &bodies, &eog, &raw).unwrap();
        assert_eq!(owner.counts_v1().vocabulary, table.vocab());
        assert_eq!(owner.counts_v1().raw_bytes, eof.actual_bytes);
        assert_eq!(owner.raw_v1(), raw);
        owner.check_scope_v1(p.source_id, claim, scope, semantics.commitment(), eof.inventory_id).unwrap();
        assert!(owner.check_scope_v1(h(996), claim, scope, semantics.commitment(), eof.inventory_id).is_err());
        assert!(store.retain_semantic_material_v1(&b.state, h(997), &bodies, &eog, &raw).is_err());
        assert!(store.retain_semantic_material_v1(&b.state, claim, &bodies[..bodies.len() - 1], &eog, &raw).is_err());
        let mut changed = raw.clone();
        changed[0] ^= 1;
        assert!(store.retain_semantic_material_v1(&b.state, claim, &bodies, &eog, &changed).is_err());
    }
    // Production data paths generated under a local target context; the final signed
    // material intent below uses a different context and rechecks the same paths.
    let rendering = table.opening_of(0).unwrap();
    let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(
        source.header_v1(),
        &fx.record,
        frame.descriptor_v1().maximum_leaves,
    )
    .unwrap();
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &claim).unwrap();
    let base = crate::palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1::new_with_operand_length_view_v1(
        h(961),
        &target,
        &view,
        &frame.original_v1().job,
        0,
        rendering.rendering.is_some(),
        rendering.rendering.as_ref().map_or(0, Vec::len) as u32,
        &rendering.opening,
        eof.actual_bytes,
    )
    .unwrap();
    let produced = crate::palw_tokenizer_table_stream_v1::producer::generate(
        &base,
        &raw,
        crate::palw_tokenizer_table_stream_v1::producer::Budget {
            max_proofs: 1000,
            max_nodes: 513000,
            max_requests: 1 << 30,
            max_hash_calls: 1000000,
            max_hash_bytes: 1 << 30,
        },
    )
    .unwrap();
    let proofs = produced.proofs();
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
    let all_encoded = (0..table.vocab()).map(|id| borsh::to_vec(&table.opening_of(id).unwrap()).unwrap()).collect::<Vec<_>>();
    let all_bodies = all_encoded.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let eog_encoded = borsh::to_vec(&table.eog_opening().unwrap()).unwrap();
    let retained = store.retain_semantic_material_v1(&b.state, claim, &all_bodies, &eog_encoded, &raw).unwrap();
    let rooted_retained = store.retain_semantic_material_v1(&b.state, claim, &[], &[], &[]).unwrap();
    assert_eq!(rooted_retained.counts_v1(), retained.counts_v1());
    let rooted_evidence =
        store.build_tokenizer_terminal_retained_v1(&b.state, &params, &rules, &intent, &rooted_retained, proofs, budget).unwrap();
    assert_eq!(
        rooted_evidence.recheck(&store, &b.state, &params, &rules, budget).unwrap(),
        evidence.recheck(&store, &b.state, &params, &rules, budget).unwrap()
    );
    include!("palw_court_tokenizer_family_native_test_block.rs");
    let retained_evidence =
        store.build_tokenizer_terminal_retained_v1(&b.state, &params, &rules, &intent, &retained, proofs, budget).unwrap();
    assert_eq!(
        retained_evidence.recheck(&store, &b.state, &params, &rules, budget).unwrap(),
        evidence.recheck(&store, &b.state, &params, &rules, budget).unwrap()
    );
    let mut wrong = intent.clone();
    wrong.inventory_id = h(966);
    sign(&mut wrong);
    assert!(store.build_tokenizer_terminal_retained_v1(&b.state, &params, &rules, &wrong, &retained, proofs, budget).is_err());
    let mut wrong = intent.clone();
    wrong.scope_id = h(967);
    sign(&mut wrong);
    assert!(store.build_tokenizer_terminal_retained_v1(&b.state, &params, &rules, &wrong, &retained, proofs, budget).is_err());
    let mut wrong = intent.clone();
    wrong.token_id = table.vocab();
    sign(&mut wrong);
    assert!(store.build_tokenizer_terminal_retained_v1(&b.state, &params, &rules, &wrong, &retained, proofs, budget).is_err());
    assert!(
        store
            .build_tokenizer_terminal_retained_v1(&b.state, &params, &rules, &intent, &retained, &proofs[..proofs.len() - 1], budget)
            .is_err()
    );
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

#[test]
fn prospective_tokenizer_cache_rechecks_rooted_eof_and_claim_scope() {
    use crate::palw_tokenizer_rendering_v2::{PalwTokenizerRenderingBudgetV2, PalwTokenizerRenderingCacheV2};

    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    let (mut state, params, extras) = world(&fx, &fx.frame);
    let claim = fx.frame.plan_v1().original_claim;
    let class = semantics.class_id;
    let accepted_daa = state.claim(&claim).unwrap().accepted_daa;
    assert!(accepted_daa > eof.completed_daa);
    state.class_semantics.insert(class, semantics.clone());
    state.claim_semantics.insert(claim, semantics);
    let eog = table.eog_opening().unwrap();
    let token = table.opening_of(0).unwrap();
    let limits = PalwTokenizerRenderingBudgetV2 {
        raw_bytes: eof.actual_bytes,
        vocab: table.vocab(),
        key_events: eof.key_event_upper_bound_v1(),
        eog_ids: 1,
        total_rendering_bytes: 8,
    };
    let mut cache = PalwTokenizerRenderingCacheV2::default();
    assert!(cache.prepare_class_v2(&state, &class, accepted_daa, &raw, &eog, limits).is_err());
    assert_eq!(cache.verify_claim_token_v2(&state, &claim, 0, &token), None);

    let inventory = Key::Inventory(eof.artifact_hash);
    let mut accepted = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    accepted.write_staged_row_v1(inventory, Some(Row::ArtifactInventory(eof.clone()))).unwrap();
    assert!(cache.prepare_class_v2(&accepted.state, &class, eof.completed_daa - 1, &raw, &eog, limits).is_err());
    cache.prepare_class_v2(&accepted.state, &class, accepted_daa, &raw, &eog, limits).unwrap();
    assert!(cache.class_is_prepared_v2(&accepted.state, &class, accepted_daa));
    assert_eq!(cache.verify_claim_token_v2(&accepted.state, &claim, 0, &token), Some(true));
    assert_eq!(cache.verify_claim_token_v2(&accepted.state, &claim, 1, &token), Some(false));
    assert_eq!(cache.lowest_eog_for_claim_v2(&accepted.state, &claim), Some(7));
    assert_eq!(PalwTokenizerRenderingCacheV2::default().verify_claim_token_v2(&accepted.state, &claim, 0, &token), None);

    // No inventory on the sibling view, then a different native EOF identity.
    assert!(!cache.class_is_prepared_v2(&state, &class, accepted_daa));
    assert_eq!(cache.verify_claim_token_v2(&state, &claim, 0, &token), None);
    let mut alternate = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let other_eof = crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1 {
        inventory_id: h(8801),
        ..eof.clone()
    };
    alternate.write_staged_row_v1(inventory, Some(Row::ArtifactInventory(other_eof))).unwrap();
    assert_eq!(cache.verify_claim_token_v2(&alternate.state, &claim, 0, &token), None);

    // A claim must still have its native original and output binding. A claim
    // stamped at the EOF DAA cannot establish ordering from that DAA alone.
    let original_output = accepted.state.claim(&claim).unwrap().output_root;
    accepted.state.claims.get_mut(&claim).unwrap().output_root = h(8802);
    assert_eq!(cache.verify_claim_token_v2(&accepted.state, &claim, 0, &token), None);
    accepted.state.claims.get_mut(&claim).unwrap().output_root = original_output;
    assert_eq!(cache.verify_claim_token_v2(&accepted.state, &claim, 0, &token), Some(true));
    let original_frame = accepted.state.staged_courts.remove(&fx.frame.key_v1()).unwrap();
    assert_eq!(cache.verify_claim_token_v2(&accepted.state, &claim, 0, &token), None);
    accepted.state.staged_courts.insert(fx.frame.key_v1(), original_frame);
    let mut same_daa_eof = eof.clone();
    same_daa_eof.completed_daa = accepted_daa;
    accepted.write_staged_row_v1(inventory, Some(Row::ArtifactInventory(same_daa_eof))).unwrap();
    let mut same_daa_cache = PalwTokenizerRenderingCacheV2::default();
    same_daa_cache.prepare_class_v2(&accepted.state, &class, accepted_daa, &raw, &eog, limits).unwrap();
    assert_eq!(same_daa_cache.verify_claim_token_v2(&accepted.state, &claim, 0, &token), None);
}

#[test]
fn tfp1_tokenizer_intake_pins_native_container_and_signed_eof_without_changing_quote() {
    use crate::palw_tir_container_v1::{check_tir_container_binding_v1, test_binding_v1};
    use crate::palw_tokenizer_rendering_v2::{
        PalwTfp1TokenizerIntakePinsV2 as Pins, PalwTfp1TokenizerIntakeQuoteErrorV2 as Error, PalwTokenizerRenderingBudgetV2,
        PalwTokenizerRenderingCacheV2, PalwVerifiedTokenizerRegistrationV1,
    };

    // This fixture validates a real signed inventory offer, then drives the
    // native exact EOF primitive; the native registration check binds PALWART3.
    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    let (mut state, params, mut extras) = world(&fx, &fx.frame);
    let claim = fx.frame.plan_v1().original_claim;
    state.claims.remove(&claim);
    state.staged_courts.remove(&fx.frame.key_v1());
    state.last_point = Some(at(&fx.frame, 0));
    let now = state.last_point().unwrap().daa_score + 1;
    let class_id = semantics.class_id;
    let binding = test_binding_v1(&fx.record, semantics.artifact_root, b"tfp1-intake");
    let container = check_tir_container_binding_v1(&binding, &class_id, &semantics.artifact_root, &fx.record, 1).unwrap();
    let pins = Pins {
        class_id,
        container_root: container.container_root,
        semantics_id: semantics.commitment(),
        inventory_id: eof.inventory_id,
    };
    state.class_semantics.insert(class_id, semantics);
    let container_key = Key::TirContainer(class_id);
    let eof_key = Key::Inventory(eof.artifact_hash);
    let original = fx.frame.original_v1();
    let header = fx.frame.header_v1();
    let eog = table.eog_opening().unwrap();
    let budget = PalwTokenizerRenderingBudgetV2 {
        raw_bytes: eof.actual_bytes,
        vocab: table.vocab(),
        key_events: eof.key_event_upper_bound_v1(),
        eog_ids: 1,
        total_rendering_bytes: 8,
    };
    let mut cache = PalwTokenizerRenderingCacheV2::default();
    let full_quote = |state: &PalwChainStateV2, extras: &PalwTransitionExtrasV1| {
        super::palw_tir_public_original_quote_v1(state, &params, extras, now, original, header)
    };
    let preclaim_quote = |cache: &PalwTokenizerRenderingCacheV2, state: &PalwChainStateV2, extras: &PalwTransitionExtrasV1| {
        cache.tfp1_intake_quote_v2(state, &params, extras, now, &pins, original, header)
    };
    let quote_outcome = |res| match res {
        Ok(plan) => Ok(plan),
        Err(Error::Native(error)) => Err(error),
        Err(Error::Local(reason)) => panic!("unexpected local intake error {reason}"),
    };
    assert_eq!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &pins, &[], &eog, budget), Ok(false));
    assert_eq!(quote_outcome(preclaim_quote(&cache, &state, &extras)), full_quote(&state, &extras));

    extras.class_semantics_active = true;
    extras.tir_container_identity_active = true;
    extras.tir_public_conviction_active = true;
    extras.staged_public_court = Some(fx.rules.config);
    assert!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &pins, &raw, &eog, budget).is_err());
    state.staged_courts.insert(eof_key, Row::ArtifactInventory(eof));
    assert!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &pins, &raw, &eog, budget).is_err());
    state.staged_courts.insert(container_key, Row::TirContainer(Box::new(container)));
    assert!(matches!(preclaim_quote(&cache, &state, &extras), Err(Error::Local(_))));
    for wrong in
        [Pins { inventory_id: h(8804), ..pins }, Pins { container_root: h(8805), ..pins }, Pins { semantics_id: h(8806), ..pins }]
    {
        assert!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &wrong, &raw, &eog, budget).is_err());
    }
    assert!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &pins, &raw[..raw.len() - 1], &eog, budget).is_err());
    assert!(matches!(preclaim_quote(&cache, &state, &extras), Err(Error::Local(_))));
    assert_eq!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &pins, &raw, &eog, budget), Ok(true));
    assert_eq!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &pins, &[], &eog, budget), Ok(true));
    let too_small = PalwTokenizerRenderingBudgetV2 { eog_ids: 0, ..budget };
    assert!(cache.prepare_tfp1_intake_v2(&state, &extras, now, &pins, &raw, &eog, too_small).is_err());
    let derive =
        |parent: &PalwChainStateV2, bytes: &[u8], eog_opening: &crate::palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1| {
            PalwVerifiedTokenizerRegistrationV1::derive_v1(parent, &extras, now, pins, bytes, eog_opening, budget)
        };
    let record = derive(&state, &raw, &eog).unwrap().unwrap();
    let restored = derive(&state, &raw, &eog).unwrap().unwrap();
    assert_eq!(record.commitment_v1(), restored.commitment_v1());
    assert!(record.applies_to_parent_v1(&state, &extras, now, budget));
    assert!(!record.applies_to_parent_v1(&state, &extras, now, PalwTokenizerRenderingBudgetV2 { eog_ids: 2, ..budget }));
    let mut bad_eog = eog.clone();
    bad_eog.ids[0] = 0;
    assert!(derive(&state, &raw, &bad_eog).is_err());
    assert!(derive(&state, &raw[..raw.len() - 1], &eog).is_err());
    assert_eq!(quote_outcome(preclaim_quote(&cache, &state, &extras)), full_quote(&state, &extras));
    let mut wrong_header = header.clone();
    wrong_header.artifact_root = h(8807);
    assert!(matches!(cache.tfp1_intake_quote_v2(&state, &params, &extras, now, &pins, original, &wrong_header), Err(Error::Local(_))));
    state.staged_courts.remove(&container_key);
    assert!(!record.applies_to_parent_v1(&state, &extras, now, budget));
    assert!(derive(&state, &raw, &eog).is_err());
    assert!(matches!(preclaim_quote(&cache, &state, &extras), Err(Error::Local(_))));
}

#[test]
fn verified_tokenizer_registration_root_replay_and_carriage_require_native_context() {
    use crate::palw_native_resource_v1::{NativeHeapV1, NativeWireReaderV1, NativeWireScanV1};
    use crate::palw_state_v2::{PalwDeltaEntryV2, PalwStateCarriageV2, PalwStateDeltaV2, apply_delta_v2, revert_delta_v2};
    use crate::palw_tir_container_v1::{check_tir_container_binding_v1, test_binding_v1};
    use crate::palw_tokenizer_rendering_v2::{
        PalwTfp1TokenizerIntakePinsV2, palw_rooted_tokenizer_opening_bounds_v1, palw_rooted_tokenizer_registration_v1,
    };

    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    let (mut base, params, mut extras) = world(&fx, &fx.frame);
    let class = semantics.class_id;
    let binding = test_binding_v1(&fx.record, semantics.artifact_root, b"rooted-tokenizer");
    let container = check_tir_container_binding_v1(&binding, &class, &semantics.artifact_root, &fx.record, 1).unwrap();
    let pins = PalwTfp1TokenizerIntakePinsV2 {
        class_id: class,
        container_root: container.container_root,
        semantics_id: semantics.commitment(),
        inventory_id: eof.inventory_id,
    };
    let prior = at(&fx.frame, 0);
    let point = at(&fx.frame, 1);
    base.last_point = Some(prior);
    base.class_semantics.insert(class, semantics);
    let eof_key = Key::Inventory(eof.artifact_hash);
    let mut prerequisites = TransitionBuilder::new(&base, &params, false, false, false, false, &extras);
    prerequisites.write_staged_row_v1(eof_key, Some(Row::ArtifactInventory(eof))).unwrap();
    prerequisites.write_staged_row_v1(Key::TirContainer(class), Some(Row::TirContainer(Box::new(container)))).unwrap();
    let parent = prerequisites.state;
    let eog = table.eog_opening().unwrap();
    let mut dormant = TransitionBuilder::new(&parent, &params, false, false, false, false, &extras);
    assert!(dormant.install_verified_tokenizer_registration_v1(&parent, point, pins, &raw, &eog).is_err());
    assert!(dormant.entries.is_empty());

    extras.class_semantics_active = true;
    extras.tir_container_identity_active = true;
    extras.tir_public_conviction_active = true;
    extras.staged_public_court = Some(fx.rules.config);
    let mut b = TransitionBuilder::new(&parent, &params, false, false, false, false, &extras);
    let mut bad_eog = eog.clone();
    bad_eog.ids[0] = 0;
    assert!(b.install_verified_tokenizer_registration_v1(&parent, point, pins, &raw, &bad_eog).is_err());
    assert!(b.entries.is_empty());
    b.install_verified_tokenizer_registration_v1(&parent, point, pins, &raw, &eog).unwrap();
    let key = Key::VerifiedTokenizerRegistration(class, pins.semantics_id);
    assert!(b.install_verified_tokenizer_registration_v1(&parent, point, pins, &raw, &eog).is_err());
    let row = b.state.staged_court_row_v1(&key).unwrap();
    let bytes = borsh::to_vec(row).unwrap();
    assert_eq!(bytes[0], 38);
    assert_eq!(borsh::to_vec(&key).unwrap()[0], 31);
    assert!(bytes.len() < 1536);
    let mut scan = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
    assert_eq!(Row::scan_native_wire_v1(&mut scan).unwrap().heap, row.native_heap_v1("verified tokenizer fixture").bytes().unwrap());
    assert_eq!(scan.remaining(), 0);
    assert!(borsh::from_slice::<Row>(&bytes[..bytes.len() - 1]).is_err());
    let decoded = borsh::from_slice::<Row>(&bytes).unwrap();
    let mut bypass = TransitionBuilder::new(&parent, &params, false, false, false, false, &extras);
    assert!(bypass.write_staged_row_v1(key, Some(decoded.clone())).is_err());
    assert!(bypass.entries.is_empty());

    b.state.last_point = Some(point);
    b.entries.push(PalwDeltaEntryV2::LastPoint { old: Some(prior), new: Some(point) });
    let delta = PalwStateDeltaV2 { point, entries: b.entries };
    let next = apply_delta_v2(&parent, &delta, &params).unwrap();
    assert_eq!(next.state_root(), b.state.state_root());
    assert_eq!(revert_delta_v2(&next, &delta, &params).unwrap().state_root(), parent.state_root());
    assert!(palw_rooted_tokenizer_registration_v1(&parent, &extras, class, point.daa_score + 1).is_none());
    assert!(palw_rooted_tokenizer_registration_v1(&next, &extras, class, point.daa_score).is_none());
    let record = palw_rooted_tokenizer_registration_v1(&next, &extras, class, point.daa_score + 1).unwrap();
    assert_eq!(record.facts_v1().eog_count, 1);
    assert_eq!(record.facts_v1().lowest_eog_id, 7);
    let quote = |state, daa, original| palw_rooted_tokenizer_opening_bounds_v1(state, &extras, daa, original, fx.frame.header_v1());
    let original = fx.frame.original_v1();
    assert!(quote(&parent, point.daa_score + 1, original).is_none());
    assert!(quote(&next, point.daa_score, original).is_none());
    let bounds = quote(&next, point.daa_score + 1, original).unwrap();
    assert_eq!(bounds.registration_id, record.record_id_v1());
    assert_eq!(bounds.eog_opening_bytes, borsh::object_length(&eog).unwrap() as u64);
    let mut changed = original.clone();
    changed.job.tokenizer_id = Hash64::from_u64_word(778);
    assert!(quote(&next, point.daa_score + 1, &changed).is_none());
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&borsh::to_vec(&PalwStateCarriageV2::from_state(&next)).unwrap())
        .unwrap()
        .into_state(&params, Some(next.state_root()))
        .unwrap();
    assert_eq!(palw_rooted_tokenizer_registration_v1(&restored, &extras, class, point.daa_score + 1), Some(record));
    assert_eq!(quote(&restored, point.daa_score + 1, original), Some(bounds));

    let mut prefix_filled = restored.clone();
    prefix_filled.class_semantics.get_mut(&class).unwrap().prefix_state_relation =
        Some(crate::palw_class_semantics_v1::PalwPrefixStateRelationV1::DenseInt32KvV1);
    let Row::VerifiedTokenizerRegistration(historical) = prefix_filled.staged_court_row_v1(&key).unwrap() else { unreachable!() };
    historical.check_context(&prefix_filled).unwrap();
    assert!(palw_rooted_tokenizer_registration_v1(&prefix_filled, &extras, class, point.daa_score + 1).is_none());

    let mut foreign_parent = parent.clone();
    foreign_parent.last_point.as_mut().unwrap().block = crate::BlockHash::from_u64_word(8808);
    assert!(apply_delta_v2(&foreign_parent, &delta, &params).is_err());
    let mut foreign_point = delta.clone();
    foreign_point.point.block = crate::BlockHash::from_u64_word(8809);
    assert!(apply_delta_v2(&parent, &foreign_point, &params).is_err());
    let mut deleted = delta.clone();
    deleted.entries[0] = PalwDeltaEntryV2::StagedCourtSeed { key, old: Some(decoded), new: None };
    assert!(apply_delta_v2(&next, &deleted, &params).is_err());
    let mut orphan = PalwStateCarriageV2::from_state(&next);
    orphan.staged_courts.remove(&eof_key);
    assert!(orphan.into_state(&params, None).is_err());
}

#[test]
fn rooted_tokenizer_stop_material_pins_original_ids_rendering_and_eog() {
    use crate::palw_constraint_stream_court_v1::canonical_replay::Material as _;
    use crate::palw_court_shared_material_intake_v1::batch::plain_tokenizer::PlainBatchTokenizerV2;
    use crate::palw_court_shared_tir_builder_store_v1::{
        constraint_stop::RootedStopMaterialV1, constraint_terminal::ConstraintMaterialV1,
    };
    use crate::palw_tir_container_v1::{check_tir_container_binding_v1, test_binding_v1};
    use crate::palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, palw_rooted_claim_tokenizer_v1};

    let companion_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_with_executor_fixture_v1(
        companion_key.verification_key.as_ref(),
    );
    let (mut accepted, params, mut extras) = world(&fx, &fx.frame);
    let claim = fx.frame.plan_v1().original_claim;
    let class = semantics.class_id;
    let binding = test_binding_v1(&fx.record, semantics.artifact_root, b"rooted-tokenizer-original-ids");
    let container = check_tir_container_binding_v1(&binding, &class, &semantics.artifact_root, &fx.record, 1).unwrap();
    let pins = PalwTfp1TokenizerIntakePinsV2 {
        class_id: class,
        container_root: container.container_root,
        semantics_id: semantics.commitment(),
        inventory_id: eof.inventory_id,
    };
    let eof_key = Key::Inventory(eof.artifact_hash);
    accepted.class_semantics.insert(class, semantics.clone());
    accepted.claim_semantics.insert(claim, semantics);
    accepted.staged_courts.insert(eof_key, Row::ArtifactInventory(eof));
    accepted.staged_courts.insert(Key::TirContainer(class), Row::TirContainer(Box::new(container)));
    accepted.last_point = Some(at(&fx.frame, 0));
    let mut registration_parent = accepted.clone();
    registration_parent.claims.remove(&claim);
    registration_parent.claim_semantics.remove(&claim);
    registration_parent.staged_courts.remove(&fx.frame.key_v1());
    registration_parent.last_point =
        Some(PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(8810), daa_score: 10, blue_score: 10, subsidy: 0 });
    let registration_point =
        PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(8811), daa_score: 11, blue_score: 11, subsidy: 0 };
    extras.class_semantics_active = true;
    extras.tir_container_identity_active = true;
    extras.tir_public_conviction_active = true;
    extras.staged_public_court = Some(fx.rules.config);
    let mut b = TransitionBuilder::new(&registration_parent, &params, false, false, false, false, &extras);
    let eog = table.eog_opening().unwrap();
    b.install_verified_tokenizer_registration_v1(&registration_parent, registration_point, pins, &raw, &eog).unwrap();
    let key = Key::VerifiedTokenizerRegistration(class, pins.semantics_id);
    let source = fx.frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
    let at = PalwBlockContextV2 {
        daa_score: registration_point.daa_score + 1,
        blue_score: registration_point.blue_score + 1,
        ..registration_point
    };
    let original = fx.frame.original_v1();
    let form = crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
    let derive =
        |state: &PalwChainStateV2, at: &PalwBlockContextV2, original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3| {
            PlainBatchTokenizerV2::derive_v2(state, &extras, at, original, &source, form)
        };
    assert!(derive(&registration_parent, &at, original).is_err(), "EOF-only parent cannot authorize reuse");
    assert!(derive(&b.state, &at, original).is_err(), "unaccepted in-flight registration cannot authorize reuse");
    b.state.last_point = Some(registration_point);
    assert!(derive(&b.state, &PalwBlockContextV2 { daa_score: registration_point.daa_score, ..at }, original).is_err());
    assert!(derive(&b.state, &PalwBlockContextV2 { blue_score: registration_point.blue_score, ..at }, original).is_err());
    let branch = derive(&b.state, &at, original).unwrap();
    crate::palw_court_shared_material_intake_v1::batch::plain_output::projection::exercise_batch_booking_before_allocation_v2(
        &b.state, &extras, &params, &at, &fx,
    );
    crate::palw_court_shared_material_intake_v1::batch::admission::plain_tokenizer::check_compiled_tokenizer_fixture_v2(
        &branch, original, &source,
    );
    let rebased =
        derive(&b.state, &PalwBlockContextV2 { daa_score: at.daa_score + 1, blue_score: at.blue_score + 1, ..at }, original).unwrap();
    assert_eq!(branch.branch_id_v2(), rebased.branch_id_v2());
    assert!(branch.matches_acceptance_v2(at.daa_score));
    assert!(!branch.matches_acceptance_v2(at.daa_score + 1));
    assert!(rebased.matches_acceptance_v2(at.daa_score + 1));
    let mut foreign_original = original.clone();
    foreign_original.trace_root = Hash64::from_u64_word(991);
    assert!(derive(&b.state, &at, &foreign_original).is_err());
    branch.check_replayed_input_ids_v2(&fx.execution.prompt).unwrap();
    use crate::palw_court_shared_material_intake_v1::batch::tokenizer_transport::{
        TokenizerMaterialKindV2, exercise_tokenizer_transport_fixture_v2,
    };
    let input_body = branch.encode_input_material_v2(&fx.execution.prompt).unwrap();
    let input_received = exercise_tokenizer_transport_fixture_v2(&branch, TokenizerMaterialKindV2::InputIds, &input_body);
    let input_hash = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&input_received);
    assert_eq!(branch.decode_input_material_v2(&input_received, input_hash).unwrap().as_slice(), fx.execution.prompt);
    let mut foreign_prompt = fx.execution.prompt.clone();
    foreign_prompt[0] = (foreign_prompt[0] + 1) % source.vocab_v1();
    assert!(branch.check_replayed_input_ids_v2(&foreign_prompt).is_err());

    let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
    let ids = source.retain_complete_ids_v1(rows, &fx.execution.generated).unwrap();
    assert!(palw_rooted_claim_tokenizer_v1(&accepted, claim, &source, &ids).unwrap().is_none());
    accepted.staged_courts.insert(key, b.state.staged_court_row_v1(&key).unwrap().clone());

    let accepted_point = accepted.last_point.replace(registration_point);
    assert!(crate::palw_tokenizer_rendering_v2::palw_claim_tokenizer_registration_v1(&accepted, claim).is_err());
    assert!(crate::palw_tokenizer_rendering_v2::palw_original_tokenizer_registration_for_fold_v2(&accepted, claim).unwrap().is_some());
    accepted.last_point = accepted_point;

    let renderings: Vec<_> = ids.ids_v1().iter().map(|&id| table.opening_of(id).unwrap()).collect();
    assert!(borsh::object_length(&(&eog, &renderings)).unwrap() as u64 <= branch.bounds_v2().opening_material_bytes);
    assert_eq!(branch.bounds_v2().eog_opening_bytes, borsh::object_length(&eog).unwrap() as u64);
    branch.verify_output_openings_v2(&accepted, &source, &ids, &renderings, &eog).unwrap();
    assert!(
        branch.encode_opening_material_v2(&accepted, &source, &ids, &renderings, &eog).is_err(),
        "off-state authenticated IDs are insufficient"
    );
    // This row is a codec fixture. It does not exercise paid FullIds intake.
    let ids_key = Key::CourtSharedIds(claim);
    accepted.staged_courts.insert(
        ids_key,
        Row::CourtSharedIds(Box::new(crate::palw_court_shared_tir_state_v1::SharedTirIdsV1 {
            version: 1,
            claim,
            scope_id: Hash64::from_u64_word(901),
            rows_root: rows,
            ids: ids.ids_v1().to_vec(),
            block: Hash64::from_u64_word(906),
            accepted_daa: fx.frame.plan_v1().terms.accepted_daa,
            material_until_daa: fx.frame.plan_v1().terms.material_until_daa,
        })),
    );
    let opening_body = branch.encode_opening_material_v2(&accepted, &source, &ids, &renderings, &eog).unwrap();
    let opening_received = exercise_tokenizer_transport_fixture_v2(&branch, TokenizerMaterialKindV2::OutputOpenings, &opening_body);
    let opening_hash = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&opening_received);
    let decoded = branch.decode_opening_material_v2(&accepted, &source, &ids, &opening_received, opening_hash).unwrap();
    assert_eq!(decoded.eog, eog);
    assert_eq!(decoded.renderings, renderings);
    let saved_ids = accepted.staged_courts.remove(&ids_key).unwrap();
    assert!(branch.decode_opening_material_v2(&accepted, &source, &ids, &opening_received, opening_hash).is_err());
    accepted.staged_courts.insert(ids_key, saved_ids);
    assert!(branch.verify_output_openings_v2(&accepted, &source, &ids, &renderings[..renderings.len() - 1], &eog).is_err());
    let mut other_renderings = renderings.clone();
    other_renderings[0] = table.opening_of((ids.ids_v1()[0] + 1) % source.vocab_v1()).unwrap();
    assert!(branch.verify_output_openings_v2(&accepted, &source, &ids, &other_renderings, &eog).is_err());

    let verifier = palw_rooted_claim_tokenizer_v1(&accepted, claim, &source, &ids).unwrap().unwrap();
    let id = ids.ids_v1()[0];
    let opening = table.opening_of(id).unwrap();
    let wrong_id = table.opening_of((id + 1) % table.vocab()).unwrap();
    assert_eq!(verifier.committed_rendering_at(0, &opening), Some(opening.rendering.as_deref()));
    assert_eq!(verifier.committed_rendering_at(0, &wrong_id), None);
    assert_eq!(verifier.committed_rendering_at(source.decode_count_v1(), &opening), None);
    let material = ConstraintMaterialV1 { eog: &eog, renderings: std::slice::from_ref(&opening) };
    let replay_view = RootedStopMaterialV1::new(material, &verifier).unwrap();
    assert_eq!(replay_view.eog(), Some(eog.ids.as_slice()));
    assert_eq!(replay_view.rendering(id).unwrap().bytes, opening.rendering.as_deref());
    assert!(replay_view.rendering(wrong_id.id).is_none());
    let mut bad_eog = eog.clone();
    bad_eog.ids[0] = 0;
    assert!(branch.verify_output_openings_v2(&accepted, &source, &ids, &renderings, &bad_eog).is_err());
    assert!(RootedStopMaterialV1::new(ConstraintMaterialV1 { eog: &bad_eog, ..material }, &verifier).is_err());
    let mut bad_rendering = opening;
    bad_rendering.rendering = Some(b"unregistered".to_vec());
    assert!(RootedStopMaterialV1::new(ConstraintMaterialV1 { eog: &eog, renderings: &[bad_rendering] }, &verifier).is_err());

    let other = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 4);
    let other_source = other.frame.source_v1(&other.record, &other.layout, other.rules.maximum_leaves).unwrap();
    let other_rows =
        crate::palw_step_refute::tiled_logits_rows_root_v1(&other.execution.binding.job_context, &other.execution.rows).unwrap();
    let other_ids = other_source.retain_complete_ids_v1(other_rows, &other.execution.generated).unwrap();
    assert!(palw_rooted_claim_tokenizer_v1(&accepted, claim, &source, &other_ids).is_err());
    assert!(branch.verify_output_openings_v2(&accepted, &source, &other_ids, &renderings, &eog).is_err());
    assert!(palw_rooted_claim_tokenizer_v1(&accepted, claim, &other_source, &ids).is_err());
    accepted.claims.get_mut(&claim).unwrap().accepted_daa = registration_point.daa_score;
    assert!(palw_rooted_claim_tokenizer_v1(&accepted, claim, &source, &ids).unwrap().is_none());
    assert!(crate::palw_tokenizer_rendering_v2::palw_original_tokenizer_registration_for_fold_v2(&accepted, claim).unwrap().is_none());
    accepted.claims.get_mut(&claim).unwrap().accepted_daa = fx.frame.plan_v1().terms.accepted_daa;
    accepted.staged_courts.remove(&eof_key);
    assert!(palw_rooted_claim_tokenizer_v1(&accepted, claim, &source, &ids).is_err());
}

#[test]
fn funded_tokenizer_registration_native_open_parts_finish_and_expiry() {
    exercise_funded_tokenizer_registration(false);
}

#[test]
fn funded_tokenizer_registration_native_releases_no_op_reservations() {
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
    const CHILD: &str = "MISAKA_TEST_TKR_NO_OP_NATIVE_RESERVATION";
    if std::env::var_os(CHILD).is_none() {
        let name = format!("{}::funded_tokenizer_registration_native_releases_no_op_reservations", module_path!());
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 1 << 30, database_bytes: 0 }));
    exercise_funded_tokenizer_registration(true);
}

fn exercise_funded_tokenizer_registration(check_reservations: bool) {
    use crate::palw_state_v2::{
        PalwBlockWorkV3, PalwDeltaEntryV2, PalwStateDeltaV2, PalwStateV2Error, TokenizerRegistrationFeeWorkingStateV1, apply_delta_v2,
        apply_palw_transition_retained_v7, palw_tokenizer_registration_open_quote_v1, palw_tokenizer_registration_status_v1,
        palw_v2_apply_one_tokenizer_registration_fee_retained_v1, palw_v2_apply_one_tokenizer_registration_owned_v1,
        palw_v2_apply_one_tokenizer_registration_retained_v1, palw_v2_pre_object_base_retained_v1,
        palw_v2_pre_tokenizer_registration_base_retained_v1, revert_delta_v2,
    };
    use crate::palw_tir_container_v1::{check_tir_container_binding_v1, test_binding_v1};
    use crate::palw_tokenizer_registration_carriage_v1::*;
    use crate::palw_tokenizer_registration_state_v1::{TokenizerRegistrationStatusV1, packet_id};
    use crate::palw_tokenizer_registration_v1::*;
    use crate::palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingStatementV2};
    use crate::tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction,
    };
    let (fx, semantics, eof, raw, table) = crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    let (mut base, params, mut extras) = world(&fx, &fx.frame);
    extras.class_semantics_active = true;
    extras.tir_container_identity_active = true;
    extras.tir_public_conviction_active = true;
    extras.staged_public_court = Some(fx.rules.config);
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([166; 32]);
    base.bonds.get_mut(&outsider()).unwrap().pubkey = keypair.verification_key.as_ref().to_vec();
    let class = semantics.class_id;
    let binding = test_binding_v1(&fx.record, semantics.artifact_root, b"funded-tokenizer");
    let container = check_tir_container_binding_v1(&binding, &class, &semantics.artifact_root, &fx.record, 1).unwrap();
    let pins = PalwTfp1TokenizerIntakePinsV2 {
        class_id: class,
        container_root: container.container_root,
        semantics_id: semantics.commitment(),
        inventory_id: eof.inventory_id,
    };
    let (policy, mut body) = crate::palw_tokenizer_registration_v1::tests::fixture();
    // The shared world has a synthetic licensed claim at offset 3.
    let point = at(&fx.frame, 5);
    body.pins = pins;
    body.artifact_root = semantics.artifact_root;
    body.statement = PalwTokenizerRenderingStatementV2 {
        tokenizer_id: semantics.tokenizer_id,
        exact_raw_bytes: raw.len() as u64,
        table_root: semantics.token_table_root.unwrap(),
        vocab: fx.record.logits_vocab,
        rule: semantics.rendering_rule.unwrap(),
    };
    let eog = table.eog_opening().unwrap();
    body.eog_digest = TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap();
    body.opener = outsider();
    extras.tokenizer_registration_policy_v1 = Some(policy);
    extras.tokenizer_registration_open_cutoff_daa_v1 = Some(point.daa_score + 1);
    extras.tokenizer_registration_network_domain_v1 = Some(body.network);
    body.accept_until_daa = point.daa_score + 5;
    body.finish_until_daa = point.daa_score + 40;
    body.material_until_daa = body.finish_until_daa + policy.retention_daa;
    let quote = policy.quote_v1(&body, point.daa_score, semantics.tokenizer_key_events.unwrap()).unwrap();
    let open_id = body.id_v1().unwrap();
    base.last_point = Some(at(&fx.frame, 4));
    // The shared fixture seeds primary claim rows for direct fold tests.
    // A full transition requires the corresponding parent deadline index.
    crate::palw_state_v2::rebuild_deadline_index_v2(&mut base, &params).unwrap();
    base.assert_deadline_consistency(&params).unwrap();
    base.class_semantics.insert(class, semantics);
    let mut prerequisite = TransitionBuilder::new(&base, &params, false, false, false, false, &extras);
    prerequisite.write_staged_row_v1(Key::Inventory(eof.artifact_hash), Some(Row::ArtifactInventory(eof))).unwrap();
    prerequisite.write_staged_row_v1(Key::TirContainer(class), Some(Row::TirContainer(Box::new(container)))).unwrap();
    let mut state = prerequisite.state;
    assert_eq!(borsh::object_length(&body).unwrap(), TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1);
    let root_before_quote = state.state_root();
    let quoted = palw_tokenizer_registration_open_quote_v1(&state, &params, &point, &extras, &body).unwrap().unwrap();
    assert_eq!((quoted.open_id, quoted.policy, quoted.quote), (open_id, policy, quote));
    assert_eq!(state.state_root(), root_before_quote);
    assert!(palw_tokenizer_registration_status_v1(&state, open_id).unwrap().is_none());
    let mut dormant = extras.clone();
    dormant.tokenizer_registration_policy_v1 = None;
    assert!(palw_tokenizer_registration_open_quote_v1(&state, &params, &point, &dormant, &body).unwrap().is_none());
    // This body can be priced but there is insufficient time to reserve its
    // Part, normal Finish and recovery Finish at strictly increasing DAAs.
    let short_open = TokenizerRegistrationOpenBodyV1 {
        accept_until_daa: point.daa_score + 1,
        finish_until_daa: point.daa_score + 3,
        material_until_daa: point.daa_score + 3 + policy.retention_daa,
        ..body
    };
    assert!(policy.quote_v1(&short_open, point.daa_score, quoted.declared_key_events).is_ok());
    assert!(matches!(
        palw_tokenizer_registration_open_quote_v1(&state, &params, &point, &extras, &short_open),
        Err(PalwStateV2Error::CarriageInconsistent(reason)) if reason.contains("capacity")
    ));
    let mut open = TokenizerRegistrationOpenV1 { body, signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] };
    let open_message = open.signing_message_v1().unwrap();
    open.signature.copy_from_slice(
        libcrux_ml_dsa::ml_dsa_87::sign(
            &keypair.signing_key,
            open_message.as_byte_slice(),
            TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1,
            [167; 32],
        )
        .unwrap()
        .as_ref(),
    );
    let prepare = |state: &PalwChainStateV2, point: PalwBlockContextV2, wire: &TokenizerRegistrationWireV1, paid: u64| {
        let prevout = TransactionOutpoint::new(h(point.blue_score + 80_000), 0);
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(prevout, vec![], 0, 0)],
            vec![TransactionOutput::new(10, ScriptPublicKey::default())],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            encode_tokenizer_registration_payload_v1(wire).unwrap(),
        );
        let populated = PopulatedTransaction::new(&tx, vec![UtxoEntry::new(paid + 10, ScriptPublicKey::default(), 0, false)]);
        let validated = ValidatedTransaction::new(populated, paid);
        let context = TokenizerRegistrationCarrierContextV1 {
            parent_root: state.state_root(),
            at: point,
            accepted_ordinal: 0,
            network: body.network,
            policy: body.policy,
        };
        ValidatedTokenizerRegistrationCarrierV1::prepare_shared_v1(&validated, context, &policy, 0).unwrap().unwrap()
    };
    let rehearse = |parent: &PalwChainStateV2, carrier: &ValidatedTokenizerRegistrationCarrierV1| {
        let context = carrier.context_v1();
        let prefix = palw_v2_pre_object_base_retained_v1(parent, &params, &context.at, false, false, false, false, &extras).unwrap();
        palw_v2_apply_one_tokenizer_registration_retained_v1(
            &prefix,
            parent,
            context.parent_root,
            &params,
            &context.at,
            carrier,
            false,
            false,
            false,
            false,
            &extras,
        )
        .unwrap()
    };
    let rehearse_owned = |parent: &PalwChainStateV2, carrier: &ValidatedTokenizerRegistrationCarrierV1| {
        let context = carrier.context_v1();
        let prefix = palw_v2_pre_object_base_retained_v1(parent, &params, &context.at, false, false, false, false, &extras).unwrap();
        palw_v2_apply_one_tokenizer_registration_owned_v1(
            prefix,
            parent,
            context.parent_root,
            &params,
            &context.at,
            carrier,
            false,
            false,
            false,
            false,
            &extras,
        )
        .unwrap()
    };
    let rehearse_fee = |parent: &PalwChainStateV2, carrier: &ValidatedTokenizerRegistrationCarrierV1| {
        let context = carrier.context_v1();
        let prefix = palw_v2_pre_tokenizer_registration_base_retained_v1(parent, &params, &context.at, &extras).unwrap();
        palw_v2_apply_one_tokenizer_registration_fee_retained_v1(
            &prefix,
            parent,
            context.parent_root,
            &params,
            &context.at,
            carrier,
            &extras,
        )
        .unwrap()
    };
    let actual = |parent: &PalwChainStateV2, carrier: &SharedValidatedTokenizerRegistrationCarrierV1, expected| {
        let mut authoritative = extras.clone();
        authoritative.tokenizer_registration_carriers_v1.push((0, carrier.clone()));
        authoritative.tokenizer_registration_expected_fees_v1.insert(carrier.carrier_v1(), expected);
        apply_palw_transition_retained_v7(
            parent,
            &params,
            None,
            &carrier.context_v1().at,
            &[],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            &authoritative,
        )
    };
    let apply = |state: &PalwChainStateV2, point: PalwBlockContextV2, wire: &TokenizerRegistrationWireV1, paid: u64| {
        let carrier = prepare(state, point, wire, paid);
        let context = carrier.context_v1();
        let mut b = TransitionBuilder::new(state, &params, false, false, false, false, &extras);
        let result = b
            .apply_tokenizer_registration_carrier_v1(state, context.parent_root, context, policy, carrier.carrier_v1(), &carrier)
            .unwrap();
        b.state.last_point = Some(point);
        b.entries.push(PalwDeltaEntryV2::LastPoint { old: state.last_point().copied(), new: Some(point) });
        let delta = PalwStateDeltaV2 { point, entries: b.entries };
        let next = apply_delta_v2(state, &delta, &params).unwrap();
        assert_eq!(next.state_root(), b.state.state_root());
        assert_eq!(revert_delta_v2(&next, &delta, &params).unwrap().state_root(), state.state_root());
        crate::palw_tokenizer_registration_snapshot_v1::check_tokenizer_registration_snapshot_v1(&next, |bond, until, key| {
            next.staged_holds.contains(&(bond, until, key))
        })
        .unwrap();
        (next, result)
    };
    let wire = TokenizerRegistrationWireV1::Open(open);
    let (unfunded, (fee, reason)) = apply(&state, point, &wire, quote.charge - 1);
    assert!(reason.is_some());
    assert_eq!(fee.registration_burn, 0);
    assert!(unfunded.staged_court_row_v1(&Key::TokenizerRegistrationSession(open_id)).is_none());
    let (next, (fee, reason)) = apply(&state, point, &wire, quote.charge + 7);
    assert_eq!(reason, None);
    assert_eq!((fee.registration_burn, fee.collectible_fee), (quote.charge, 7));
    let handoff = prepare(&state, point, &wire, quote.charge + 7);
    let mut wrong_domain = extras.clone();
    wrong_domain.tokenizer_registration_network_domain_v1 = Some(h(98_999));
    wrong_domain.tir_public_network_domain_v1 = Some(body.network);
    assert!(matches!(
        palw_v2_apply_one_tokenizer_registration_retained_v1(
            &state, &state, handoff.context_v1().parent_root, &params, &point, &handoff,
            false, false, false, false, &wrong_domain,
        ),
        Err(PalwStateV2Error::CarriageInconsistent(reason)) if reason == "tokenizer handoff changes native accepting point or network"
    ));
    let (rehearsed, expected, refused) = rehearse(&state, &handoff);
    assert_eq!(refused, None);
    assert_eq!(expected, fee);
    let (authoritative, _, _) = actual(&state, &handoff, expected).unwrap();
    let session_key = Key::TokenizerRegistrationSession(open_id);
    assert!(authoritative.staged_court_row_v1(&session_key).is_some());
    assert_eq!(authoritative.staged_court_row_v1(&session_key), rehearsed.staged_court_row_v1(&session_key));
    let accepted_status = palw_tokenizer_registration_status_v1(&authoritative, open_id).unwrap().unwrap();
    // Hostile snapshot row counts fail before any cross-row/hold traversal.
    // A counterfeit per-session policy cannot expand the priced book bounds.
    {
        let check = |s: &PalwChainStateV2| {
            crate::palw_tokenizer_registration_snapshot_v1::check_tokenizer_registration_snapshot_v1(s, |_, _, _| {
                panic!("over-budget snapshot must fail before checking holds")
            })
        };
        let original = authoritative.staged_court_row_v1(&session_key).unwrap().clone();
        let mut excessive = (*authoritative).clone();
        for index in 0..=policy.max_pending_total {
            excessive.staged_courts.insert(Key::TokenizerRegistrationSession(h(110_000 + u64::from(index))), original.clone());
        }
        assert_eq!(check(&excessive).unwrap_err(), "tokenizer snapshot row count exceeds its work envelope");
        let mut foreign = (*authoritative).clone();
        let Row::TokenizerRegistrationSession(mut session) = original else { unreachable!() };
        session.policy.max_key_events += 1;
        foreign.staged_courts.insert(session_key, Row::TokenizerRegistrationSession(session));
        assert_eq!(check(&foreign).unwrap_err(), "tokenizer session substitutes the snapshot work policy");
    }
    assert_eq!(accepted_status.open, body);
    assert_eq!(accepted_status.accepted, point);
    assert_eq!(accepted_status.charge_id, expected.charge_id.unwrap());
    assert_eq!(accepted_status.quote, quote);
    assert_eq!(accepted_status.status, TokenizerRegistrationStatusV1::Pending);
    assert!(accepted_status.released_at.is_none());
    assert!(accepted_status.parts.iter().all(|part| part.accepted.is_none() && part.slot_daa > point.daa_score));
    assert!(accepted_status.finishes.iter().all(|finish| !finish.attempted));
    let duplicate_point = PalwBlockContextV2 { block: h(98_999), blue_score: point.blue_score + 1, ..point };
    assert!(matches!(
        palw_tokenizer_registration_open_quote_v1(&authoritative, &params, &duplicate_point, &extras, &body),
        Err(PalwStateV2Error::CarriageInconsistent(reason)) if reason.contains("already retained")
    ));
    let (isolated, isolated_fee, refusal) = rehearse_fee(&state, &handoff);
    assert_eq!((isolated_fee, refusal), (expected, None));
    for key in [session_key, Key::TokenizerRegistrationBook] {
        assert_eq!(isolated.staged_court_row_v1(&key), authoritative.staged_court_row_v1(&key));
    }
    // A body whose gross fee exceeds its recipient's reward capacity may
    // still fit after the independently derived Open service burn. A rejected
    // trial must leave no reservation that makes a later retry look prepaid.
    {
        use crate::palw_bond_budget_v1::{
            PalwBondBudgetPolicyV1, PalwBondBudgetStateV1, PalwRoundFeeAdmissionV1, PalwRoundFeeIncomeV1,
        };
        let budget_policy = PalwBondBudgetPolicyV1::unapproved_probe_v1();
        let mut budget = PalwRoundFeeAdmissionV1 {
            policy: budget_policy.clone(),
            book: PalwBondBudgetStateV1::new(point.daa_score, &budget_policy),
            capital: [(body.opener, 7)].into_iter().collect(),
            daa_score: point.daa_score,
        };
        let income =
            |amount| PalwRoundFeeIncomeV1 { block: point.block, bond: body.opener, maximum_sompi: amount, actual_sompi: amount };
        assert!(budget.admit(income(expected.actual_fee)).is_err());
        let mut scratch = TokenizerRegistrationFeeWorkingStateV1::new(&state, &params, &point, &extras).unwrap();
        let original_root = scratch.state_v1().state_root();
        let hold = (body.opener, body.material_until_daa, session_key);
        assert!(!scratch.state_v1().staged_holds.contains(&hold));
        let trial = |scratch: &mut TokenizerRegistrationFeeWorkingStateV1| {
            scratch.apply_v1(&state, handoff.context_v1().parent_root, &params, &point, &handoff, &extras).unwrap()
        };
        scratch.begin_body_v1(&params, &extras).unwrap();
        let (projected, reason) = trial(&mut scratch);
        assert_eq!((projected, reason), (expected, None));
        assert!(scratch.state_v1().staged_holds.contains(&hold));
        let mut exhausted = budget.clone();
        exhausted.capital.insert(body.opener, 6);
        assert!(exhausted.admit(income(projected.collectible_fee)).is_err());
        scratch.begin_body_v1(&params, &extras).unwrap();
        assert_eq!(scratch.state_v1().state_root(), original_root);
        assert_eq!(scratch.state_v1().staged_holds, state.staged_holds);
        assert!(scratch.state_v1().staged_court_row_v1(&session_key).is_none());
        assert!(scratch.state_v1().staged_court_row_v1(&Key::TokenizerRegistrationBook).is_none());
        let (retried, reason) = trial(&mut scratch);
        assert_eq!((retried, reason), (projected, None));
        budget.admit(income(retried.collectible_fee)).unwrap();
        scratch.commit_body_v1();
        scratch.begin_body_v1(&params, &extras).unwrap();
        assert!(scratch.state_v1().staged_holds.contains(&hold));
        let (duplicate, reason) = trial(&mut scratch);
        assert_eq!(
            (duplicate.registration_burn, duplicate.collectible_fee, duplicate.charge_id, reason),
            (0, expected.actual_fee, None, None)
        );
        let (accepted, _, _) = actual(&state, &handoff, retried).unwrap();
        assert_eq!(accepted.staged_court_row_v1(&session_key), authoritative.staged_court_row_v1(&session_key));
    }
    let (owned, owned_fee, owned_refusal) = rehearse_owned(&state, &handoff);
    assert_eq!((owned_fee, owned_refusal), (expected, refused));
    assert_eq!(owned.state_root(), rehearsed.state_root());
    let wrong_fee = TokenizerRegistrationFeeV1 { collectible_fee: expected.collectible_fee + 1, ..expected };
    assert!(matches!(
        actual(&state, &handoff, wrong_fee),
        Err(PalwStateV2Error::CarriageInconsistent(reason)) if reason == "tokenizer fee projection differs on actual accepted prefix"
    ));
    state = next;
    let repeated = PalwBlockContextV2 { block: h(99_001), daa_score: point.daa_score + 1, blue_score: point.blue_score + 1, ..point };
    let (next, (fee, reason)) = apply(&state, repeated, &wire, quote.charge + 7);
    assert_eq!(reason, None);
    assert_eq!((fee.registration_burn, fee.collectible_fee), (0, quote.charge + 7));
    assert_eq!(state.staged_court_row_v1(&session_key), next.staged_court_row_v1(&session_key));
    assert_eq!(state.staged_court_row_v1(&Key::TokenizerRegistrationBook), next.staged_court_row_v1(&Key::TokenizerRegistrationBook));
    let TokenizerRegistrationWireV1::Open(mut late_open) = wire.clone() else { panic!() };
    late_open.body.nonce = h(99_002);
    let late_id = late_open.body.id_v1().unwrap();
    let late_message = late_open.signing_message_v1().unwrap();
    late_open.signature.copy_from_slice(
        libcrux_ml_dsa::ml_dsa_87::sign(
            &keypair.signing_key,
            late_message.as_byte_slice(),
            TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1,
            [170; 32],
        )
        .unwrap()
        .as_ref(),
    );
    let late_handoff = prepare(&state, repeated, &TokenizerRegistrationWireV1::Open(late_open), quote.charge + 7);
    let (rehearsed, fee, refused) = rehearse(&state, &late_handoff);
    assert_eq!(refused.as_deref(), Some("tokenizer registration is closed for new Open carriers"));
    assert_eq!((fee.registration_burn, fee.collectible_fee), (0, quote.charge + 7));
    assert!(rehearsed.staged_court_row_v1(&Key::TokenizerRegistrationSession(late_id)).is_none());
    let (authoritative, _, _) = actual(&state, &late_handoff, fee).unwrap();
    assert!(authoritative.staged_court_row_v1(&Key::TokenizerRegistrationSession(late_id)).is_none());
    state = next;
    let mut indices: Vec<_> = (0..quote.geometry.part_count).collect();
    let Some(Row::TokenizerRegistrationBook(schedule)) = state.staged_court_row_v1(&Key::TokenizerRegistrationBook) else { panic!() };
    indices.sort_by_key(|index| schedule.assigned_slot(open_id, 0, *index));
    let mut invalid_raw_branch = state.clone();
    for index in indices {
        let Some(Row::TokenizerRegistrationBook(book)) = state.staged_court_row_v1(&Key::TokenizerRegistrationBook) else { panic!() };
        let daa = book.book.assigned_daa(&packet_id(open_id, 0, index).unwrap()).unwrap();
        let (offset, len) = quote.geometry.part_v1(index).unwrap();
        let bytes = raw[offset as usize..offset as usize + len].to_vec();
        let mut part = TokenizerRegistrationPartV1 {
            body: TokenizerRegistrationPartBodyV1 {
                version: 1,
                network: body.network,
                policy: body.policy,
                open_id,
                opener: body.opener,
                index,
                offset,
                payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(&bytes).unwrap(),
            },
            signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            bytes,
        };
        let sign_part = |part: &mut TokenizerRegistrationPartV1| {
            let part_message = part.signing_message_v1().unwrap();
            part.signature.copy_from_slice(
                libcrux_ml_dsa::ml_dsa_87::sign(
                    &keypair.signing_key,
                    part_message.as_byte_slice(),
                    TOKENIZER_REGISTRATION_PART_CONTEXT_V1,
                    [168; 32],
                )
                .unwrap()
                .as_ref(),
            );
        };
        sign_part(&mut part);
        let mut invalid = part.clone();
        if index == 0 {
            invalid.bytes[0] ^= 1;
            invalid.body.payload_digest = TokenizerRegistrationPartV1::payload_digest_v1(&invalid.bytes).unwrap();
            sign_part(&mut invalid);
        }
        let point = PalwBlockContextV2 {
            block: h(100_000 + index as u64),
            daa_score: daa,
            blue_score: state.last_point().unwrap().blue_score + 1,
            subsidy: 0,
        };
        let (invalid_next, (_, refusal)) = apply(&invalid_raw_branch, point, &TokenizerRegistrationWireV1::Part(invalid), 7);
        assert_eq!(refusal, None);
        invalid_raw_branch = invalid_next;
        let (next, (fee, reason)) = apply(&state, point, &TokenizerRegistrationWireV1::Part(part), 7);
        assert_eq!(reason, None);
        assert_eq!(fee.registration_burn, 0);
        let status = palw_tokenizer_registration_status_v1(&next, open_id).unwrap().unwrap();
        let delivered = &status.parts[index as usize];
        assert_eq!(
            (delivered.index, delivered.offset, delivered.bytes, delivered.slot_daa, delivered.accepted),
            (index, offset, len as u32, daa, Some(point))
        );
        state = next;
    }
    let Some(Row::TokenizerRegistrationBook(book)) = state.staged_court_row_v1(&Key::TokenizerRegistrationBook) else { panic!() };
    let daa = book.book.assigned_daa(&packet_id(open_id, 1, 0).unwrap()).unwrap();
    let point =
        PalwBlockContextV2 { block: h(101_000), daa_score: daa, blue_score: state.last_point().unwrap().blue_score + 1, subsidy: 0 };
    let mut finish = TokenizerRegistrationFinishV1 {
        body: TokenizerRegistrationFinishBodyV1 {
            version: 1,
            network: body.network,
            policy: body.policy,
            open_id,
            opener: body.opener,
            eog_digest: body.eog_digest,
            finish_until_daa: body.finish_until_daa,
        },
        signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        eog,
    };
    let finish_message = finish.signing_message_v1().unwrap();
    finish.signature.copy_from_slice(
        libcrux_ml_dsa::ml_dsa_87::sign(
            &keypair.signing_key,
            finish_message.as_byte_slice(),
            TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1,
            [169; 32],
        )
        .unwrap()
        .as_ref(),
    );
    let finish_wire = TokenizerRegistrationWireV1::Finish(finish);
    let rejected_finish = prepare(&invalid_raw_branch, point, &finish_wire, 7);
    let (rehearsed, fee, refusal) = rehearse(&invalid_raw_branch, &rejected_finish);
    assert!(refusal.is_some());
    assert_eq!((fee.registration_burn, fee.collectible_fee), (0, 7));
    let Some(Row::TokenizerRegistrationSession(session)) = rehearsed.staged_court_row_v1(&session_key) else { panic!() };
    assert_eq!(session.status, TokenizerRegistrationStatusV1::Pending);
    assert_eq!(session.finish_attempts, std::collections::BTreeSet::from([0]));
    let registered = Key::VerifiedTokenizerRegistration(class, pins.semantics_id);
    assert!(rehearsed.staged_court_row_v1(&registered).is_none());
    let (owned, owned_fee, owned_refusal) = rehearse_owned(&invalid_raw_branch, &rejected_finish);
    assert_eq!(owned.state_root(), rehearsed.state_root());
    assert_eq!((owned_fee, owned_refusal.as_deref()), (fee, refusal.as_deref()));
    let (authoritative, _, _) = actual(&invalid_raw_branch, &rejected_finish, fee).unwrap();
    assert_eq!(authoritative.staged_court_row_v1(&session_key), rehearsed.staged_court_row_v1(&session_key));
    let (isolated, isolated_fee, isolated_refusal) = rehearse_fee(&invalid_raw_branch, &rejected_finish);
    assert_eq!((isolated_fee, isolated_refusal.as_deref()), (fee, refusal.as_deref()));
    for key in [session_key, Key::TokenizerRegistrationBook, registered] {
        assert_eq!(isolated.staged_court_row_v1(&key), authoritative.staged_court_row_v1(&key));
    }

    {
        let mut scratch = TokenizerRegistrationFeeWorkingStateV1::new(&invalid_raw_branch, &params, &point, &extras).unwrap();
        let original_root = scratch.state_v1().state_root();
        let original_session = scratch.state_v1().staged_court_row_v1(&session_key).cloned();
        let memo = crate::palw_tokenizer_finish_derivation_cache_v1::TokenizerFinishDerivationCacheV1::new(
            &invalid_raw_branch,
            point,
            &policy,
        )
        .unwrap();
        let trial = |scratch: &mut TokenizerRegistrationFeeWorkingStateV1| {
            memo.scope_v1(|| scratch.apply_v1(&invalid_raw_branch, memo.parent_root_v1(), &params, &point, &rejected_finish, &extras))
                .unwrap()
        };
        scratch.begin_body_v1(&params, &extras).unwrap();
        let (projected, reason) = trial(&mut scratch);
        assert_eq!((projected, reason.as_deref()), (fee, refusal.as_deref()));
        assert_eq!(scratch.state_v1().staged_court_row_v1(&session_key), rehearsed.staged_court_row_v1(&session_key));
        scratch.begin_body_v1(&params, &extras).unwrap();
        assert_eq!(scratch.state_v1().state_root(), original_root);
        assert_eq!(scratch.state_v1().staged_court_row_v1(&session_key), original_session.as_ref());
        let (retried, reason) = trial(&mut scratch);
        assert_eq!((retried, reason.as_deref()), (fee, refusal.as_deref()));
        scratch.commit_body_v1();
        scratch.begin_body_v1(&params, &extras).unwrap();
        assert_eq!(scratch.state_v1().staged_court_row_v1(&session_key), rehearsed.staged_court_row_v1(&session_key));
    }

    let handoff = prepare(&state, point, &finish_wire, 7);
    let memo =
        crate::palw_tokenizer_finish_derivation_cache_v1::TokenizerFinishDerivationCacheV1::new(&state, point, &policy).unwrap();
    let (isolated, isolated_fee, isolated_refusal) = memo.scope_v1(|| rehearse_fee(&state, &handoff));
    assert_eq!(isolated_refusal, None);
    let (authoritative, _, _) = memo.scope_v1(|| actual(&state, &handoff, isolated_fee)).unwrap();
    if check_reservations {
        let mut builder = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
        let checkpoint = |builder: &TransitionBuilder<'_>| {
            (builder.entries.len(), builder.entries.capacity(), builder.native_reservation.requests_v1())
        };
        assert!(builder.native_reservation.is_tracking_v1());
        let original = checkpoint(&builder);
        let TokenizerRegistrationWireV1::Finish(mut wrong_signature) = finish_wire.clone() else { unreachable!() };
        wrong_signature.signature[0] ^= 1;
        let wrong = prepare(&state, point, &TokenizerRegistrationWireV1::Finish(wrong_signature), 7);
        for _ in 0..3 {
            let (_, refusal) = builder.apply_tokenizer_registration_at_v1(&state, memo.parent_root_v1(), &point, &wrong).unwrap();
            assert!(refusal.is_some());
            assert_eq!(checkpoint(&builder), original);
        }
        let (completed_fee, refused) =
            memo.scope_v1(|| builder.apply_tokenizer_registration_at_v1(&state, memo.parent_root_v1(), &point, &handoff).unwrap());
        assert_eq!((completed_fee, refused), (isolated_fee, None));
        let completed = checkpoint(&builder);
        assert!(completed.0 > original.0 && completed.2 > original.2);
        for _ in 0..3 {
            let (duplicate_fee, refused) =
                builder.apply_tokenizer_registration_at_v1(&state, memo.parent_root_v1(), &point, &handoff).unwrap();
            assert_eq!((duplicate_fee, refused), (completed_fee, None));
            assert_eq!(checkpoint(&builder), completed);
        }
        assert_eq!(builder.state.staged_court_row_v1(&registered), authoritative.staged_court_row_v1(&registered));

        let mut failed = TransitionBuilder::try_new(&invalid_raw_branch, &params, false, false, false, false, &extras).unwrap();
        let original = checkpoint(&failed);
        let (_, refusal) = failed
            .apply_tokenizer_registration_at_v1(
                &invalid_raw_branch,
                rejected_finish.context_v1().parent_root,
                &point,
                &rejected_finish,
            )
            .unwrap();
        assert!(refusal.is_some());
        let attempted = checkpoint(&failed);
        assert!(attempted.0 > original.0 && attempted.2 > original.2);
    }
    drop(memo);
    for key in [session_key, Key::TokenizerRegistrationBook, registered] {
        assert_eq!(isolated.staged_court_row_v1(&key), authoritative.staged_court_row_v1(&key));
    }
    let (next, (fee, reason)) = apply(&state, point, &finish_wire, 7);
    assert_eq!(reason, None);
    assert_eq!(fee.registration_burn, 0);
    state = next;
    let Some(Row::TokenizerRegistrationSession(session)) = state.staged_court_row_v1(&Key::TokenizerRegistrationSession(open_id))
    else {
        panic!()
    };
    assert!(matches!(session.status, TokenizerRegistrationStatusV1::Complete { .. }));
    assert!(session.finish_attempts.contains(&0));
    assert!(state.staged_court_row_v1(&registered).is_some());
    let completed = palw_tokenizer_registration_status_v1(&state, open_id).unwrap().unwrap();
    assert_eq!(completed.status, session.status);
    assert_eq!(completed.released_at, Some(point));
    assert_eq!(completed.finishes[0].slot_daa, point.daa_score);
    assert!(completed.finishes[0].attempted);
    assert!(!completed.finishes[1].attempted);
    assert!(completed.parts.iter().all(|part| part.accepted.is_some()));
    let gc_point = PalwBlockContextV2 {
        block: h(102_000),
        daa_score: body.material_until_daa,
        blue_score: state.last_point().unwrap().blue_score + 1,
        subsidy: 0,
    };
    let expiry_point = PalwBlockContextV2 {
        block: h(102_001),
        daa_score: body.finish_until_daa,
        blue_score: invalid_raw_branch.last_point().unwrap().blue_score + 1,
        subsidy: 0,
    };
    let expiry = palw_v2_pre_tokenizer_registration_base_retained_v1(&invalid_raw_branch, &params, &expiry_point, &extras).unwrap();
    let Some(Row::TokenizerRegistrationSession(expired)) = expiry.staged_court_row_v1(&session_key) else { panic!() };
    assert_eq!(expired.status, TokenizerRegistrationStatusV1::Expired { point: expiry_point });
    assert!(expiry.staged_court_row_v1(&Key::TokenizerRegistrationPart(open_id, 0)).is_some());

    let isolated = palw_v2_pre_tokenizer_registration_base_retained_v1(&state, &params, &gc_point, &extras).unwrap();
    let prepared = palw_v2_pre_object_base_retained_v1(&state, &params, &gc_point, false, false, false, false, &extras).unwrap();
    let mut generic = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    super::super::palw_staged_court_fold_v1::sweep(&mut generic, &gc_point).unwrap();
    assert_eq!(isolated.last_point(), state.last_point());
    for key in [session_key, Key::TokenizerRegistrationBook, Key::TokenizerRegistrationPart(open_id, 0), registered] {
        assert_eq!(generic.state.staged_court_row_v1(&key), state.staged_court_row_v1(&key));
        assert_eq!(isolated.staged_court_row_v1(&key), prepared.staged_court_row_v1(&key));
    }
    assert!(isolated.staged_court_row_v1(&session_key).is_none());
    assert!(isolated.staged_court_row_v1(&Key::TokenizerRegistrationPart(open_id, 0)).is_none());

    let mut gc = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    gc.sweep_tokenizer_registrations_v1(&gc_point).unwrap();
    gc.state.last_point = Some(gc_point);
    gc.entries.push(PalwDeltaEntryV2::LastPoint { old: state.last_point().copied(), new: Some(gc_point) });
    let delta = PalwStateDeltaV2 { point: gc_point, entries: gc.entries };
    let expired = apply_delta_v2(&state, &delta, &params).unwrap();
    assert!(expired.staged_court_row_v1(&Key::TokenizerRegistrationSession(open_id)).is_none());
    assert!(expired.staged_court_row_v1(&Key::TokenizerRegistrationBook).is_none());
    assert!(palw_tokenizer_registration_status_v1(&expired, open_id).unwrap().is_none());
    assert!(expired.staged_court_row_v1(&registered).is_some());
    assert_eq!(revert_delta_v2(&expired, &delta, &params).unwrap().state_root(), state.state_root());
}

include!("palw_tokenizer_registration_measurement.rs");

include!("palw_batch_plain_output_native_tests_v2.rs");
