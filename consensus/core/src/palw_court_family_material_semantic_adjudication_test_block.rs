{
// Runs after actual TFO2 admission, every role's signed ingress, and cold
// restore. There is no TinyExecution evidence fallback and no source rewrite.
use crate::palw_court_shared_tir_builder_store_v1::{selection_terminal as selection, output_terminal as output};
use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as Graph, SourceGraphBudgetV3 as GraphBudget};
use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
let semantic_state = &restored;
let before = semantic_state.state_root();
let original = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(semantic_state, claim).unwrap();
let context = crate::palw_court_shared_tir_state_v1::context(semantic_state, claim).unwrap();
// Reconstruct from this restored original with the native constructor's
// pre-allocation quote and construction-peak lease, retained through every
// graph/duty/terminal below. Source intentionally has no blanket NativeHeap.
context.check(&Key::CourtSharedContext(claim)).unwrap();
context.check_binding(semantic_state.court_service_state_v1().unwrap()).unwrap();
let plan = original.plan_v1();
assert_eq!((context.allocation_id, context.source_id, context.rules_id,
    context.original_accepted_daa, context.material_until_daa),
    (plan.allocation_id, plan.source_id, plan.rules_id,
        plan.terms.accepted_daa, plan.terms.material_until_daa));
let class = original.original_v1().job.class_id;
let source_record = semantic_state.tir_class_v1(&class).unwrap();
let source_layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(semantic_state, class).unwrap();
let native_source = original.source_identity_native_v2(source_record, source_layout,
    original.descriptor_v1().maximum_leaves).unwrap();
let native_requests = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(original, &native_source, context.profile_id).unwrap();
assert_eq!((context.scope_id, context.queries), (native_requests.scope_id().unwrap(), native_requests.len()));
assert_eq!(native_source.source_id_v1(), source.source_id_v1());
let native = crate::palw_court_close_receipt_v1::NativeCourtRulesV1 {
    network_domain: network, at: *semantic_state.last_point().unwrap(), court: fx.rules.court,
    step_ladder: fx.rules.maximum_leaves,
    prompt_ids_merkle: matches!(fx.rules.prompt_form, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1),
    held_regime: false, decode_close_convicts_only: false, fp_decode_from_daa: None,
    cell_closure_from_daa: None, memory_routing_from_daa: None, tir_active: true,
    gen_active: false, task_heads_active: false, improvement_active: false, constraint_active: original.original_v1().job.is_constraint(),
};
let policy = crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&native).unwrap();
let transport = requests.signed_data_transport_upper().unwrap();
let edges = transport.fresh_responses.checked_mul(transport.maximum_custodians + 1).unwrap()
    .checked_add(transport.streams.checked_mul(2).unwrap()).unwrap().checked_add(transport.parts).unwrap();
let graph = Graph::derive_native_v3(original, &native_source, profile, GraphBudget {
    max_queries: requests.len(), max_roles: transport.maximum_custodians,
    max_vertices: transport.signed_controls, max_edges: edges,
}).unwrap();
let retained = crate::palw_court_rooted_numerical_material_v1::RootedNumericalMaterialV1::retain_v1(semantic_state, claim).unwrap();
let store = retained.store_v1(semantic_state, claim, context.scope_id).unwrap();
let Some(crate::palw_step_refute::PalwDecodeTokenPinV1::TiledV1(pin)) = store.decode_pin() else {
    panic!("real restored FullIds must be available for semantic adjudication")
};
assert_eq!(pin.generated_token_ids.len(), native_source.decode_count_v1() as usize);
if original.original_v1().job.is_constraint() {
    include!("palw_court_family_material_constraint_adjudication_test_block.rs");
} else {
let universe = selection::NativeSelectionUniverseV1::derive_native_v1(original, &native_source, &graph, profile).unwrap();
let visits = graph.nodes_v3().count() as u64;
for (position, committed) in pin.generated_token_ids.iter().copied().enumerate() {
    // Compare a distinct real vocabulary lane against each committed token,
    // including the history-bearing final position of the unchanged Prefix job.
    let position = u32::try_from(position).unwrap();
    let beat = (committed + 1) % native_source.vocab_v1();
    let duty = universe.project_native_v1(selection::SelectionTargetV1 { position, beat }, visits).unwrap();
    let mut intent = selection::SignedSelectionIntentV1 {
        version: 1, claim, scope_id: context.scope_id, policy_id: policy, duty_id: duty.absolute_id_v1(),
        position, beat, material_id: store.selection_material_id_v1(position, beat).unwrap(),
        filer: outsider(), signature: vec![],
    };
    let sign = |i: &mut selection::SignedSelectionIntentV1| {
        i.signature = mldsa::sign(&family_filer_key.signing_key, i.signing_message(network).unwrap().as_byte_slice(),
            selection::SELECTION_INTENT_CONTEXT_V1, [0x75; 32]).unwrap().as_ref().to_vec();
    };
    sign(&mut intent);
    let evidence = store.build_selection_terminal_v1(semantic_state, &family_params, &native, &intent, &duty).unwrap();
    assert!(matches!(evidence.recheck(semantic_state, &family_params, &native).unwrap(),
        selection::SelectionDecisionV1::PredicateHolds { .. }), "unchanged admitted job's real IDs/tiles obey native selection");
    let mut foreign = intent.clone(); foreign.scope_id = h(0x5E1EC7); sign(&mut foreign);
    assert!(store.build_selection_terminal_v1(semantic_state, &family_params, &native, &foreign, &duty).is_err());
    let mut corrupt = intent.clone(); corrupt.signature[0] ^= 1;
    assert!(store.build_selection_terminal_v1(semantic_state, &family_params, &native, &corrupt, &duty).is_err());
}
let output_duty = output::NativeOutputDutyV1::derive_native_v1(original, &native_source, &graph, profile).unwrap();
let Some(Row::CourtSharedIds(row)) = semantic_state.staged_court_row_v1(&Key::CourtSharedIds(claim)) else {
    panic!("FullIds must come from actual authenticated ingress")
};
let ids = native_source.retain_complete_ids_v1(row.rows_root, &row.ids).unwrap();
let mut stop = output::SignedRegularStopIntentV1 {
    version: 1, claim, scope_id: context.scope_id, policy_id: policy,
    duty_id: output_duty.native_regular_stop_id_v1().expect("adopted Prefix has the same native V4 stop rule"),
    material_id: output::ids_material_id_v1(&ids).unwrap(),
    decode_count: original.original_v1().decode_tokens_executed,
    stop_reason: original.original_v1().stop_reason, filer: outsider(), signature: vec![],
};
let sign_stop = |i: &mut output::SignedRegularStopIntentV1| {
    i.signature = mldsa::sign(&family_filer_key.signing_key, i.signing_message(network).unwrap().as_byte_slice(),
        output::REGULAR_STOP_CONTEXT_V1, [0x76; 32]).unwrap().as_ref().to_vec();
};
sign_stop(&mut stop);
let evidence = store.build_regular_stop_terminal_v1(semantic_state, &family_params, &native, &stop, &output_duty).unwrap();
assert_eq!(evidence.recheck(&store, semantic_state, &family_params, &native).unwrap(), output::RegularStopDecisionV1::StopRuleHolds);
let mut foreign = stop.clone(); foreign.scope_id = h(0x570F); sign_stop(&mut foreign);
assert!(store.build_regular_stop_terminal_v1(semantic_state, &family_params, &native, &foreign, &output_duty).is_err());
let mut corrupt = stop; corrupt.signature[0] ^= 1;
assert!(store.build_regular_stop_terminal_v1(semantic_state, &family_params, &native, &corrupt, &output_duty).is_err());
}
include!("palw_court_family_material_tokenizer_adjudication_test_block.rs");
assert_eq!(semantic_state.state_root(), before, "read-only positive predicates and refusals cannot certify credit, burn or settle");
}
