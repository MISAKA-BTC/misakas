// Included only inside the existing native fold cfg(test) module. This uses
// native allocation+prepare over the real program, not the [1,2,3] view fixture.
// Original pledge setup remains explicitly verified-half, not mined admission.
#[test]
fn shared_constraint_material_builder_reconstructs_real_native_original_and_rechecks() {
    use crate::palw_constraint_stream_court_v1::canonical_replay::WorkBudget;
    use crate::palw_court_shared_tir_builder_store_v1::{
        constraint_terminal::{
            ConstraintMaterialV1, SignedConstraintTerminalIntentV1 as Intent,
            CONSTRAINT_INTENT_SIGNATURE_CONTEXT_V1 as SIGNATURE_CONTEXT,
        },
        SharedTirBuilderStoreV1 as Store,
    };
    use crate::palw_court_shared_tir_state_v1 as shared;
    use crate::palw_tir_court_v1::{tir_logits_event_disclosure_v1, PalwTirTraceEventDisclosureV1};
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q, PalwTirPublicResponseV1 as R};
    let (fx, semantics, table) = crate::palw_tir_public_claim_v1::palw_tir_public_constraint_frame_fixture_v1();
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
        .any(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint));
    let (mut state, params, mut extras) = world(&fx, frame);
    state.claim_semantics.insert(claim, semantics.clone());
    state.class_semantics.insert(semantics.class_id, semantics.clone());
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([79; 32]);
    state.bonds.get_mut(&outsider()).unwrap().pubkey = key.verification_key.as_ref().to_vec();
    assert!(
        crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(&state, frame, p.terms.material_until_daa).is_err()
    );
    // Build the SAME signed semantic dependency as native original installation;
    // a service binding must never bypass its real custody-window check.
    let producer = frame.custody_v1().pledge_v1().producer;
    let producer_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
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
        [82; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    extras.staged_public_court = Some(fx.rules.config);
    extras.history_custody_pledges.insert(claim, pledge);
    let mut dependency = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let bundle = frame_semantic_bundle_v1(&dependency, frame, p.terms.accepted_daa).unwrap().unwrap();
    dependency.write_staged_row_v1(Key::Claim(claim), Some(bundle)).unwrap();
    let state = dependency.state.clone();
    crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(&state, frame, p.terms.material_until_daa).unwrap();

    let state = bind_clock_service(&state, frame, &params, &extras);
    let source=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
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
    let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
    let ids = source.retain_complete_ids_v1(rows, &fx.execution.generated).unwrap();
    let mut units = Vec::new();
    for row in 0..source.decode_count_v1() {
        let PalwTirTraceEventDisclosureV1::Tiled { row_root, row_opening, tile_lanes, tile_opening, .. } =
            tir_logits_event_disclosure_v1(&fx.execution.binding, &fx.execution.rows, &fx.execution.generated, row, 0).unwrap()
        else {
            panic!("actual tiled fixture")
        };
        units.push(
            source
                .authenticate_v1(
                    &Q::TraceTile { row, tile: 0 },
                    &R::TraceTile { row_root, row_opening, lanes: tile_lanes, opening: tile_opening },
                    Some(&ids),
                )
                .unwrap(),
        );
    }
    let store = Store::new(&b.state, claim, scope, &units, &[], Some(&ids)).unwrap();
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
        constraint_active: true,
    };
    let eog = table.eog_opening().unwrap();
    let renderings: Vec<_> = (0..table.vocab()).map(|i| table.opening_of(i).unwrap()).collect();
    let material = ConstraintMaterialV1 { eog: &eog, renderings: &renderings };
    assert!(semantics.raw_tokenizer_bytes.is_none());
    assert!(b.state.staged_tokenizer_inventory_v1(semantics.tokenizer_id).is_none());
    let encoded=renderings.iter().map(|x|borsh::to_vec(x).unwrap()).collect::<Vec<_>>();
    let bodies=encoded.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let eog_encoded=borsh::to_vec(&eog).unwrap();
    let retained=store.retain_semantic_table_material_v1(&b.state,claim,&bodies,&eog_encoded).unwrap();
    assert_eq!(retained.counts_v1().vocabulary,table.vocab());
    assert_eq!(retained.counts_v1().raw_bytes,0); // no raw body in this owner
    assert!(store.retain_semantic_table_material_v1(&b.state,claim,&bodies[..bodies.len()-1],&eog_encoded).is_err());
    let budget = WorkBudget { max_material_bytes: 1 << 25, max_calls: 100_000 };
    let mut intent = Intent {
        version: 1,
        claim,
        scope_id: scope,
        policy_id: crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&rules).unwrap(),
        semantics_id: semantics.commitment(),
        material_id: material.identity(budget).unwrap(),
        position: 0,
        beat_lane: fx.execution.generated[0],
        witness: None,
        filer: outsider(),
        signature: vec![],
    };
    let sign = |i: &mut Intent| {
        i.signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            i.signing_message(rules.network_domain).unwrap().as_byte_slice(),
            SIGNATURE_CONTEXT,
            [80; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
    };
    sign(&mut intent);
    let before = b.state.state_root();
    use crate::palw_court_shared_tir_builder_store_v1::constraint_family::{ConstraintUniverseV1 as Universe,ConstraintTargetV1 as FamilyTarget,ConstraintSemanticSupplyV1};
    use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as FamilyGraph,SourceGraphBudgetV3};
    let upper=set.signed_data_transport_upper().unwrap();
    let graph_edges=upper.fresh_responses.checked_mul(upper.maximum_custodians+1).unwrap().checked_add(upper.streams.checked_mul(2).unwrap()).unwrap().checked_add(upper.parts).unwrap();
    let family_graph=FamilyGraph::derive_native_v3(frame,&source,profile,SourceGraphBudgetV3{max_queries:set.len(),max_roles:upper.maximum_custodians,max_vertices:upper.signed_controls,max_edges:graph_edges}).unwrap();
    let family=Universe::derive_native_v1(&b.state,frame,&source,&family_graph,profile).unwrap();
    assert_eq!(family.semantic_supply_v1(),ConstraintSemanticSupplyV1::UnresolvedTableSupply{table:semantics.token_table_root.unwrap(),vocabulary:source.vocab_v1()});
    let mut rank=0;
    for position in 0..source.decode_count_v1(){for beat in 0..source.vocab_v1(){for w in 0..=source.vocab_v1(){
        assert_eq!(family.target_at_v1(rank),Some(FamilyTarget::Token{position,beat,witness:if w==0{None}else{Some(w-1)}}));rank+=1;
    }}}
    assert_eq!(family.target_at_v1(rank),Some(FamilyTarget::Stop));assert_eq!(family.len_v1(),rank+1);assert!(family.target_at_v1(rank+1).is_none());
    let visits=family_graph.nodes_v3().count() as u64;
    assert!(family.project_native_v1(FamilyTarget::Token{position:source.decode_count_v1(),beat:0,witness:None},visits).is_err());
    assert!(family.project_native_v1(FamilyTarget::Token{position:0,beat:source.vocab_v1(),witness:None},visits).is_err());
    assert!(family.project_native_v1(FamilyTarget::Token{position:0,beat:0,witness:Some(source.vocab_v1())},visits).is_err());
    let last=family.project_native_v1(FamilyTarget::Token{position:source.decode_count_v1()-1,beat:source.vocab_v1()-1,witness:Some(source.vocab_v1()-1)},visits).unwrap();
    assert_eq!(last.full_ids_dependencies_v1().len() as u64,family_graph.role_slots_v3());
    let tiles=u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    assert_eq!(last.row_dependencies_v1().len() as u64,tiles*family_graph.role_slots_v3());
    assert_ne!(last.logical_id_v1(),Hash64::default());assert_ne!(last.absolute_id_v1(),Hash64::default());
    assert!(matches!(family.project_native_v1(FamilyTarget::Token{position:0,beat:0,witness:None},0),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
    let duty=family.project_native_v1(FamilyTarget::Token{position:intent.position,beat:intent.beat_lane,witness:intent.witness},visits).unwrap();
    let checked=store.build_constraint_token_with_duty_v1(&b.state,&params,&rules,&intent,&retained,budget,&duty).unwrap();
    let stop_duty=family.project_native_v1(FamilyTarget::Stop,visits).unwrap();
    assert!(stop_duty.row_dependencies_v1().is_empty());
    assert!(store.build_constraint_token_with_duty_v1(&b.state,&params,&rules,&intent,&retained,budget,&stop_duty).is_err());
    let evidence = store.build_constraint_terminal(&b.state, &params, &rules, &intent, material, budget).unwrap();
    assert_eq!(checked.recheck(&store,&b.state,&params,&rules,budget).unwrap(),evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap());
    let selected_id=fx.execution.generated[intent.position as usize];
    assert!(!eog.ids.contains(&selected_id),"fixture requires actual selected rendering");
    let sparse_material=ConstraintMaterialV1{eog:&eog,renderings:std::slice::from_ref(&renderings[selected_id as usize])};
    let mut sparse_intent=intent.clone();sparse_intent.material_id=sparse_material.identity(budget).unwrap();sign(&mut sparse_intent);
    let sparse_evidence=store.build_constraint_token_material_with_duty_v1(&b.state,&params,&rules,&sparse_intent,sparse_material,budget,&duty).unwrap();
    assert_eq!(sparse_evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap(),evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap());
    assert!(sparse_material.renderings.len()<renderings.len(),"selected target does not require every potential rendering");
    let missing_material=ConstraintMaterialV1{eog:&eog,renderings:&[]};let mut missing_intent=intent.clone();missing_intent.material_id=missing_material.identity(budget).unwrap();sign(&mut missing_intent);
    assert!(store.build_constraint_token_material_with_duty_v1(&b.state,&params,&rules,&missing_intent,missing_material,budget,&duty).is_err());
    let retained_evidence=store.build_constraint_terminal_retained_v1(&b.state,&params,&rules,&intent,&retained,budget).unwrap();
    assert_eq!(retained_evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap(),
        evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap());
    let mut wrong=intent.clone();wrong.scope_id=h(971);sign(&mut wrong);
    assert!(store.build_constraint_terminal_retained_v1(&b.state,&params,&rules,&wrong,&retained,budget).is_err());
    let finding = evidence.recheck(&store, &b.state, &params, &rules, budget).unwrap().expect("invalid selected rendering convicted");
    assert_eq!(finding.claim_id, claim);
    assert_eq!(finding.execution_root, frame.header_v1().committed_execution_root);
    assert_eq!(b.state.state_root(), before);
    let no_ids = Store::new(&b.state, claim, scope, &units, &[], None).unwrap();
    assert!(evidence.recheck(&no_ids, &b.state, &params, &rules, budget).is_err());
    let mut changed = intent.clone();
    changed.position = 1;
    assert!(store.build_constraint_terminal(&b.state, &params, &rules, &changed, material, budget).is_err());
    let mut changed = intent.clone();
    changed.semantics_id = h(909);
    sign(&mut changed);
    assert!(store.build_constraint_terminal(&b.state, &params, &rules, &changed, material, budget).is_err());
    let mut corrupt = renderings.clone();
    corrupt[0].rendering.as_mut().unwrap()[0] ^= 1;
    let bad = ConstraintMaterialV1 { eog: &eog, renderings: &corrupt };
    let mut changed = intent.clone();
    changed.material_id = bad.identity(budget).unwrap();
    sign(&mut changed);
    assert!(store.build_constraint_terminal(&b.state, &params, &rules, &changed, bad, budget).is_err());
    let mut later = rules.clone();
    later.at.daa_score = p.terms.accepted_daa + 10_000;
    assert!(evidence.recheck(&store, &b.state, &params, &later, budget).is_err());
    let mut stale = b.state.clone();
    stale.bonds.get_mut(&outsider()).unwrap().status = PalwBondStatusV2::Retiring { since_daa: 101, settled_at_since: 0 };
    assert!(evidence.recheck(&store, &stale, &params, &rules, budget).is_err());
    assert_eq!(b.state.state_root(), before);
}

#[test]
fn shared_constraint_stop_terminal_uses_full_ids_without_row_and_preserves_boundaries(){
    use crate::palw_constraint_stream_court_v1::canonical_replay::WorkBudget;
    use crate::palw_court_shared_tir_builder_store_v1::{
        constraint_terminal::{
            ConstraintMaterialV1, SignedConstraintTerminalIntentV1 as Intent,
            CONSTRAINT_INTENT_SIGNATURE_CONTEXT_V1 as SIGNATURE_CONTEXT,
        },
        SharedTirBuilderStoreV1 as Store,
    };
    use crate::palw_court_shared_tir_state_v1 as shared;
    use crate::palw_tir_court_v1::{tir_logits_event_disclosure_v1, PalwTirTraceEventDisclosureV1};
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q, PalwTirPublicResponseV1 as R};
    use crate::palw_court_shared_tir_builder_store_v1::constraint_stop::{SignedConstraintStopIntentV1 as StopIntent,CONSTRAINT_STOP_SIGNATURE_CONTEXT_V1 as STOP_CONTEXT};
    use crate::palw_freeprompt_v3::PalwFpStopReasonV3 as Stop;
    for stop in [Stop::ExactBudgetReached,Stop::EndOfGeneration] {for fault in [false,true] {
    let (fx, semantics, table) = crate::palw_tir_public_claim_v1::palw_tir_public_constraint_stop_frame_fixture_v1(stop,fault);
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
        .any(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint));
    let (mut state, params, mut extras) = world(&fx, frame);
    state.claim_semantics.insert(claim, semantics.clone());
    state.class_semantics.insert(semantics.class_id, semantics.clone());
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([79; 32]);
    state.bonds.get_mut(&outsider()).unwrap().pubkey = key.verification_key.as_ref().to_vec();
    assert!(
        crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(&state, frame, p.terms.material_until_daa).is_err()
    );
    // Build the SAME signed semantic dependency as native original installation;
    // a service binding must never bypass its real custody-window check.
    let producer = frame.custody_v1().pledge_v1().producer;
    let producer_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
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
        [82; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    extras.staged_public_court = Some(fx.rules.config);
    extras.history_custody_pledges.insert(claim, pledge);
    let mut dependency = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let bundle = frame_semantic_bundle_v1(&dependency, frame, p.terms.accepted_daa).unwrap().unwrap();
    dependency.write_staged_row_v1(Key::Claim(claim), Some(bundle)).unwrap();
    let state = dependency.state.clone();
    crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(&state, frame, p.terms.material_until_daa).unwrap();

    let state = bind_clock_service(&state, frame, &params, &extras);
    let source=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
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
    let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
    let ids = source.retain_complete_ids_v1(rows, &fx.execution.generated).unwrap();
    let store = Store::new(&b.state, claim, scope, &[], &[], Some(&ids)).unwrap();
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
        constraint_active: true,
    };
    let eog=table.eog_opening().unwrap();
    let renderings=(0..table.vocab()).map(|id|table.opening_of(id).unwrap()).collect::<Vec<_>>();
    let material=ConstraintMaterialV1{eog:&eog,renderings:&renderings};
    let budget=WorkBudget{max_material_bytes:1<<25,max_calls:100_000};
    let mut intent=StopIntent{version:1,claim,scope_id:scope,
        policy_id:crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&rules).unwrap(),
        semantics_id:semantics.commitment(),material_id:material.identity(budget).unwrap(),position:source.decode_count_v1(),
        stop_reason:stop,filer:outsider(),signature:vec![]};
    let sign=|i:&mut StopIntent|{i.signature=libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key,
        i.signing_message(rules.network_domain).unwrap().as_byte_slice(),STOP_CONTEXT,[97;32]).unwrap().as_ref().to_vec();};
    sign(&mut intent);let before=b.state.state_root();
    let evidence=store.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&intent,material,budget).unwrap();
    assert_eq!(evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap().is_some(),fault);
    // Exact native end position; token position D-1 cannot become a stop root.
    let mut wrong=intent.clone();wrong.position-=1;sign(&mut wrong);
    assert!(store.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&wrong,material,budget).is_err());
    let mut wrong=intent.clone();wrong.position+=1;sign(&mut wrong);
    assert!(store.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&wrong,material,budget).is_err());
    let mut wrong=intent.clone();wrong.stop_reason=if stop==Stop::ExactBudgetReached{Stop::EndOfGeneration}else{Stop::ExactBudgetReached};sign(&mut wrong);
    assert!(store.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&wrong,material,budget).is_err());
    let mut wrong=intent.clone();wrong.scope_id=h(979);sign(&mut wrong);
    assert!(store.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&wrong,material,budget).is_err());
    let missing=Store::new(&b.state,claim,scope,&[],&[],None).unwrap();
    assert!(missing.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&intent,material,budget).is_err());
    let mut eog_bad=eog.clone();eog_bad.ids[0]=(eog_bad.ids[0]+1)%table.vocab();
    let bad=ConstraintMaterialV1{eog:&eog_bad,renderings:&renderings};
    let mut wrong=intent.clone();wrong.material_id=bad.identity(budget).unwrap();sign(&mut wrong);
    assert!(store.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&wrong,bad,budget).is_err());
    let encoded=renderings.iter().map(|o|borsh::to_vec(o).unwrap()).collect::<Vec<_>>();
    let bodies=encoded.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let eog_wire=borsh::to_vec(&eog).unwrap();
    let owner=store.retain_semantic_table_material_v1(&b.state,claim,&bodies,&eog_wire).unwrap();
    use crate::palw_court_shared_tir_builder_store_v1::constraint_family::{ConstraintUniverseV1 as Universe,ConstraintTargetV1 as FamilyTarget,ConstraintSemanticSupplyV1};
    use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as FamilyGraph,SourceGraphBudgetV3};
    let upper=set.signed_data_transport_upper().unwrap();
    let graph_edges=upper.fresh_responses.checked_mul(upper.maximum_custodians+1).unwrap().checked_add(upper.streams.checked_mul(2).unwrap()).unwrap().checked_add(upper.parts).unwrap();
    let family_graph=FamilyGraph::derive_native_v3(frame,&source,profile,SourceGraphBudgetV3{max_queries:set.len(),max_roles:upper.maximum_custodians,max_vertices:upper.signed_controls,max_edges:graph_edges}).unwrap();
    let family=Universe::derive_native_v1(&b.state,frame,&source,&family_graph,profile).unwrap();
    assert_eq!(family.semantic_supply_v1(),ConstraintSemanticSupplyV1::UnresolvedTableSupply{table:semantics.token_table_root.unwrap(),vocabulary:source.vocab_v1()});
    let mut rank=0;
    for position in 0..source.decode_count_v1(){for beat in 0..source.vocab_v1(){for w in 0..=source.vocab_v1(){
        assert_eq!(family.target_at_v1(rank),Some(FamilyTarget::Token{position,beat,witness:if w==0{None}else{Some(w-1)}}));rank+=1;
    }}}
    assert_eq!(family.target_at_v1(rank),Some(FamilyTarget::Stop));assert_eq!(family.len_v1(),rank+1);assert!(family.target_at_v1(rank+1).is_none());
    let visits=family_graph.nodes_v3().count() as u64;
    assert!(family.project_native_v1(FamilyTarget::Token{position:source.decode_count_v1(),beat:0,witness:None},visits).is_err());
    assert!(family.project_native_v1(FamilyTarget::Token{position:0,beat:source.vocab_v1(),witness:None},visits).is_err());
    assert!(family.project_native_v1(FamilyTarget::Token{position:0,beat:0,witness:Some(source.vocab_v1())},visits).is_err());
    let last=family.project_native_v1(FamilyTarget::Token{position:source.decode_count_v1()-1,beat:source.vocab_v1()-1,witness:Some(source.vocab_v1()-1)},visits).unwrap();
    assert_eq!(last.full_ids_dependencies_v1().len() as u64,family_graph.role_slots_v3());
    let tiles=u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    assert_eq!(last.row_dependencies_v1().len() as u64,tiles*family_graph.role_slots_v3());
    assert_ne!(last.logical_id_v1(),Hash64::default());assert_ne!(last.absolute_id_v1(),Hash64::default());
    assert!(matches!(family.project_native_v1(FamilyTarget::Token{position:0,beat:0,witness:None},0),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
    let duty=family.project_native_v1(FamilyTarget::Stop,visits).unwrap();
    assert!(duty.row_dependencies_v1().is_empty());assert_eq!(duty.semantic_supply_v1(),ConstraintSemanticSupplyV1::UnresolvedEogOnlySupply{table:semantics.token_table_root.unwrap(),vocabulary:source.vocab_v1()});assert_eq!(duty.full_ids_dependencies_v1().len() as u64,family_graph.role_slots_v3());
    let checked=store.build_constraint_stop_with_duty_v1(&b.state,&params,&rules,&intent,&owner,budget,&duty).unwrap();
    assert_eq!(checked.recheck(&store,&b.state,&params,&rules,budget).unwrap(),evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap());
    let token_duty=family.project_native_v1(FamilyTarget::Token{position:0,beat:0,witness:None},visits).unwrap();
    assert!(store.build_constraint_stop_with_duty_v1(&b.state,&params,&rules,&intent,&owner,budget,&token_duty).is_err());
    let retained=store.build_constraint_stop_terminal_retained_v1(&b.state,&params,&rules,&intent,&owner,budget).unwrap();
    assert_eq!(retained.recheck(&store,&b.state,&params,&rules,budget).unwrap(),evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap());
    let view=crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(source.header_v1(),&fx.record,fx.rules.maximum_leaves).unwrap();
    let view_identity=borsh::to_vec(source.header_v1()).unwrap();
    let direct=|candidate:&crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,actual_ids:&crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1| {
        crate::palw_constraint_stream_court_v1::PalwConstraintStreamCourtV1::new_from_tiled_ids_with_original_stop_context_v1(
            crate::palw_freeprompt_v3::fp_claim_id_v3(candidate),outsider(),p.terms.accepted_daa,&view,&view_identity,&source,actual_ids,candidate,
            eog.ids.len() as u32,&eog.opening,h(981))
    };
    // Prove that this route reaches the normative terminal-stop predicate,
    // not the deliberately false rendering/grammar arm of this fixture.
    let base=direct(frame.original_v1(),&ids).unwrap();
    use crate::palw_constraint_stream_court_v1::{PalwConstraintStreamPhaseV1 as Phase,canonical_replay as stop_replay};
    assert_eq!(base.phase(),Phase::Eog);assert!(base.rendering_cursor_v1().is_none());
    assert_eq!(base.selection_for_tests_v1(),(source.decode_count_v1(),None,false));
    struct EogOnly<'a>{ids:&'a [u32],rendering_calls:std::cell::Cell<u32>}
    impl stop_replay::Material for EogOnly<'_>{
        fn eog(&self)->Option<&[u32]>{Some(self.ids)}
        fn rendering(&self,_:u32)->Option<stop_replay::RenderingMaterial<'_>>{
            self.rendering_calls.set(self.rendering_calls.get()+1);None
        }
    }
    let only=EogOnly{ids:&eog.ids,rendering_calls:std::cell::Cell::new(0)};
    let terminal=stop_replay::build(&base,&only,budget).unwrap();
    assert_eq!(only.rendering_calls.get(),0,"stop path must never enter grammar/rendering");
    assert_eq!(terminal.work().material_bytes,eog.ids.len() as u64*4);
    assert_eq!(terminal.work().calls,2,"one EOG part and finish only");
    assert_eq!(terminal.finding().is_some(),fault);
    if let Some(found)=terminal.finding(){
        assert_eq!(found.position,if stop==Stop::ExactBudgetReached{0}else{source.decode_count_v1()-1});
    }
    let mut finished=base.clone();
    finished.push_eog(finished.seed_id(),0,&eog.ids).unwrap();finished.finish_eog().unwrap();
    assert_eq!(finished.phase(),Phase::Complete);assert!(finished.rendering_cursor_v1().is_none());
    assert_eq!(finished.fault().unwrap().is_some(),fault);
    // Signed private adapter also succeeds with ONLY authenticated EOG, so no
    // unrelated invalid-rendering result can make the positive pass by accident.
    let eog_only=ConstraintMaterialV1{eog:&eog,renderings:&[]};
    let mut end_only=intent.clone();end_only.material_id=eog_only.identity(budget).unwrap();sign(&mut end_only);
    let projected_end=store.build_constraint_stop_material_with_duty_v1(&b.state,&params,&rules,&end_only,eog_only,budget,&duty).unwrap();
    assert_eq!(projected_end.recheck(&store,&b.state,&params,&rules,budget).unwrap(),evidence.recheck(&store,&b.state,&params,&rules,budget).unwrap());
    assert!(store.build_constraint_stop_material_with_duty_v1(&b.state,&params,&rules,&end_only,eog_only,budget,&token_duty).is_err());
    let end_only=store.build_constraint_stop_terminal_v1(&b.state,&params,&rules,&end_only,eog_only,budget).unwrap();
    assert_eq!(end_only.recheck(&store,&b.state,&params,&rules,budget).unwrap().is_some(),fault);
    // Negative-only altered native facts, never installed as a valid original.
    let mut changed=frame.original_v1().clone();changed.decode_tokens_executed-=1;
    assert!(matches!(direct(&changed,&ids),Err(e) if e.contains("native original claim/count/stop facts")));
    let mut changed=frame.original_v1().clone();changed.job.decode_token_limit=changed.decode_tokens_executed-1;
    assert!(matches!(direct(&changed,&ids),Err(e) if e.contains("native original claim/count/stop facts")));
    if stop==Stop::ExactBudgetReached {
        let mut changed=frame.original_v1().clone();changed.job.decode_token_limit+=1;
        assert!(matches!(direct(&changed,&ids),Err(e) if e.contains("native original claim/count/stop facts")));
    }
    let (other,_,_)=crate::palw_tir_public_claim_v1::palw_tir_public_constraint_stop_frame_fixture_v1(stop,!fault);
    let other_source=other.frame.source_v1(&other.record,&other.layout,other.rules.maximum_leaves).unwrap();
    let other_rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&other.execution.binding.job_context,&other.execution.rows).unwrap();
    let other_ids=other_source.retain_complete_ids_v1(other_rows,&other.execution.generated).unwrap();
    assert!(matches!(direct(frame.original_v1(),&other_ids),Err(e) if e.contains("actual source/view/FullIds")));
    assert_eq!(b.state.state_root(),before);
    // Old token builder still requires an authenticated row and its own old
    // intent/domain; this row-free Stop Store cannot satisfy token selection.
    let old=Intent{version:1,claim,scope_id:scope,policy_id:intent.policy_id,semantics_id:intent.semantics_id,
        material_id:intent.material_id,position:source.decode_count_v1()-1,beat_lane:0,witness:None,filer:outsider(),signature:vec![]};
    let mut old=old;old.signature=libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key,old.signing_message(rules.network_domain).unwrap().as_byte_slice(),SIGNATURE_CONTEXT,[98;32]).unwrap().as_ref().to_vec();
    assert!(store.build_constraint_terminal(&b.state,&params,&rules,&old,material,budget).is_err());
    }}
}
