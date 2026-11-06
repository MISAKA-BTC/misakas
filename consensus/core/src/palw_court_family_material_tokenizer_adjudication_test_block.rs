{
// Real accepted EOF + supplied raw tokenizer/table, consumed by the native
// canonical replay adapter. This establishes one rendering predicate only.
use crate::palw_court_shared_tir_builder_store_v1::tokenizer_terminal as tok;
let sem = semantic_state.claim_semantics_v1(&claim).unwrap();
let eof = semantic_state.staged_tokenizer_inventory_v1(sem.tokenizer_id).expect("actual retained EOF");
let owner = store.retain_semantic_material_v1(semantic_state, claim, &[], &[], &[]).unwrap();
let raw = owner.raw_v1();
assert_eq!(raw.len() as u64, eof.actual_bytes);
assert_eq!(crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw), sem.tokenizer_id);
let rendering = owner.rendering_v1(0).unwrap();
let record = semantic_state.tir_class_v1(&original.original_v1().job.class_id).unwrap();
let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(native_source.header_v1(), record, original.descriptor_v1().maximum_leaves).unwrap();
let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(semantic_state, &claim).unwrap();
let base = crate::palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1::new_with_operand_length_view_v1(
    h(0x70C3), &target, &view, &original.original_v1().job, 0, rendering.rendering.is_some(),
    u32::try_from(rendering.rendering.as_ref().map_or(0, Vec::len)).unwrap(), &rendering.opening, eof.actual_bytes,
).unwrap();
let produced = crate::palw_tokenizer_table_stream_v1::producer::generate(&base, raw,
    crate::palw_tokenizer_table_stream_v1::producer::Budget {
        max_proofs: 1000, max_nodes: 513000, max_requests: 1 << 30, max_hash_calls: 1000000, max_hash_bytes: 1 << 30,
    }).unwrap();
let proofs = produced.proofs();
assert!(!proofs.is_empty());
let budget = crate::palw_tokenizer_table_stream_v1::canonical_replay::Budget {
    max_material_bytes: 1 << 25, max_proofs: 1000, max_native_calls: 100_000, max_packets: 10_000,
};
let material = tok::TokenizerMaterialV1 {raw, proofs, rendering};
let mut intent = tok::SignedTokenizerTerminalIntentV1 {
    version: 1, claim, scope_id: context.scope_id, policy_id: policy, semantics_id: sem.commitment(),
    inventory_id: eof.inventory_id, material_id: material.identity(budget).unwrap(), token_id: 0,
    filer: outsider(), signature: vec![],
};
let sign = |i: &mut tok::SignedTokenizerTerminalIntentV1| {
    i.signature = mldsa::sign(&family_filer_key.signing_key, i.signing_message(network).unwrap().as_byte_slice(),
        tok::TOKENIZER_INTENT_SIGNATURE_CONTEXT_V1, [0x79;32]).unwrap().as_ref().to_vec();
};
sign(&mut intent);
let evidence = store.build_tokenizer_terminal_retained_v1(semantic_state, &family_params, &native, &intent, &owner, proofs, budget).unwrap();
assert!(!evidence.recheck(&store, semantic_state, &family_params, &native, budget).unwrap(),
    "actual raw tokenizer agrees with the immutable table rendering");
let mut foreign = intent.clone(); foreign.scope_id = h(0x70C4); sign(&mut foreign);
assert!(store.build_tokenizer_terminal_retained_v1(semantic_state, &family_params, &native, &foreign, &owner, proofs, budget).is_err());
let mut corrupt = intent; corrupt.signature[0] ^= 1;
assert!(store.build_tokenizer_terminal_retained_v1(semantic_state, &family_params, &native, &corrupt, &owner, proofs, budget).is_err());
}
