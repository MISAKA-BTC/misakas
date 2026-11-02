{
use crate::palw_court_shared_tir_builder_store_v1::{constraint_terminal as ct, constraint_stop as cs, constraint_family as cf};
use crate::palw_constraint_stream_court_v1::canonical_replay::WorkBudget;
assert!(native.constraint_active);
let sem = semantic_state.claim_semantics_v1(&claim).expect("admitted original's immutable semantics");
let rooted_table = crate::palw_court_rooted_semantic_v1::palw_retain_rooted_family_table_v1(semantic_state, claim).unwrap().unwrap();
let table = rooted_table.table_v1();
assert_eq!(Some(table.root()), sem.token_table_root);
assert_eq!(crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(&original.original_v1().job), sem.token_table_root);
let eog = table.eog_opening().unwrap();
let renderings = (0..table.vocab()).map(|id| table.opening_of(id).unwrap()).collect::<Vec<_>>();
let material = ct::ConstraintMaterialV1 {eog: &eog, renderings: &renderings};
let budget = WorkBudget {max_material_bytes: 1 << 25, max_calls: 100_000};
let material_id = material.identity(budget).unwrap();
let universe = cf::ConstraintUniverseV1::derive_native_v1(semantic_state, original, &native_source, &graph, profile).unwrap();
let visits = graph.nodes_v3().count() as u64;
for (position, committed) in pin.generated_token_ids.iter().copied().enumerate() {
    let position = u32::try_from(position).unwrap();
    let duty = universe.project_native_v1(cf::ConstraintTargetV1::Token {position, beat: committed, witness: None}, visits).unwrap();
    let mut intent = ct::SignedConstraintTerminalIntentV1 {
        version: 1, claim, scope_id: context.scope_id, policy_id: policy, semantics_id: sem.commitment(), material_id,
        position, beat_lane: committed, witness: None, filer: outsider(), signature: vec![],
    };
    let sign = |i: &mut ct::SignedConstraintTerminalIntentV1| {
        i.signature = mldsa::sign(&family_filer_key.signing_key, i.signing_message(network).unwrap().as_byte_slice(),
            ct::CONSTRAINT_INTENT_SIGNATURE_CONTEXT_V1, [0x77;32]).unwrap().as_ref().to_vec();
    };
    sign(&mut intent);
    let evidence = store.build_constraint_token_material_with_duty_v1(semantic_state, &family_params, &native, &intent, material, budget, &duty).unwrap();
    assert!(evidence.recheck(&store, semantic_state, &family_params, &native, budget).unwrap().is_none(),
        "real original's constrained token obeys the authenticated grammar/table mask");
    let mut foreign = intent.clone(); foreign.scope_id = h(0xC0157); sign(&mut foreign);
    assert!(store.build_constraint_token_material_with_duty_v1(semantic_state, &family_params, &native, &foreign, material, budget, &duty).is_err());
    let mut corrupt = intent; corrupt.signature[0] ^= 1;
    assert!(store.build_constraint_token_material_with_duty_v1(semantic_state, &family_params, &native, &corrupt, material, budget, &duty).is_err());
}
let duty = universe.project_native_v1(cf::ConstraintTargetV1::Stop, visits).unwrap();
let mut intent = cs::SignedConstraintStopIntentV1 {
    version: 1, claim, scope_id: context.scope_id, policy_id: policy, semantics_id: sem.commitment(), material_id,
    position: native_source.decode_count_v1(), stop_reason: original.original_v1().stop_reason,
    filer: outsider(), signature: vec![],
};
let sign = |i: &mut cs::SignedConstraintStopIntentV1| {
    i.signature = mldsa::sign(&family_filer_key.signing_key, i.signing_message(network).unwrap().as_byte_slice(),
        cs::CONSTRAINT_STOP_SIGNATURE_CONTEXT_V1, [0x78;32]).unwrap().as_ref().to_vec();
};
sign(&mut intent);
let evidence = store.build_constraint_stop_material_with_duty_v1(semantic_state, &family_params, &native, &intent, material, budget, &duty).unwrap();
assert!(evidence.recheck(&store, semantic_state, &family_params, &native, budget).unwrap().is_none(),
    "real original stop reason agrees with its immutable constrained job and actual FullIds");
let mut foreign = intent.clone(); foreign.scope_id = h(0xC0570F); sign(&mut foreign);
assert!(store.build_constraint_stop_material_with_duty_v1(semantic_state, &family_params, &native, &foreign, material, budget, &duty).is_err());
let mut corrupt = intent; corrupt.signature[0] ^= 1;
assert!(store.build_constraint_stop_material_with_duty_v1(semantic_state, &family_params, &native, &corrupt, material, budget, &duty).is_err());
}
