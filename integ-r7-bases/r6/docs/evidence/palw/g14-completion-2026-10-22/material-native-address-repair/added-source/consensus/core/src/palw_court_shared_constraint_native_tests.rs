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
    let evidence = store.build_constraint_terminal(&b.state, &params, &rules, &intent, material, budget).unwrap();
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
