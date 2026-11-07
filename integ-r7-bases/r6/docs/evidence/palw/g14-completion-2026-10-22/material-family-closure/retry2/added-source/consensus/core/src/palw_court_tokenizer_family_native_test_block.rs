{
    use crate::palw_court_shared_tir_builder_store_v1::tokenizer_family::{TokenizerFamilyV1 as Family,TokenizerContextRequirementV1};
    let source_owner=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
    let required=std::cell::Cell::new(0);
    let family=Family::derive_reserved_v1(&b.state,frame,&source_owner,&retained,&produced,|q|{required.set(q);Ok(None)}).unwrap();
    assert_eq!(produced.artifact_binding_v1(),(semantics.tokenizer_id,raw.len() as u64));
    assert_eq!(produced.key_work_bound_v1(),semantics.tokenizer_key_events.unwrap());
    // A true native Court under a different qualified statement can seal the
    // same exact raw bytes with fewer key proofs. It is not current authority.
    let mut lower_target=target.clone();lower_target.registered_semantics.as_mut().unwrap().tokenizer_key_events=Some(0);
    let lower_base=crate::palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1::new_with_operand_length_view_v1(
        h(0xDAB2),&lower_target,&view,&frame.original_v1().job,0,rendering.rendering.is_some(),rendering.rendering.as_ref().map_or(0,Vec::len) as u32,&rendering.opening,eof.actual_bytes).unwrap();
    let lower=crate::palw_tokenizer_table_stream_v1::producer::generate(&lower_base,&raw,
        crate::palw_tokenizer_table_stream_v1::producer::Budget{max_proofs:1000,max_nodes:513000,max_requests:1<<30,max_hash_calls:1000000,max_hash_bytes:1<<30}).unwrap();
    assert_eq!(lower.artifact_binding_v1(),produced.artifact_binding_v1());assert_eq!(lower.key_work_bound_v1(),0);assert_ne!(lower.key_work_bound_v1(),produced.key_work_bound_v1());
    assert!(lower.proofs().len()<produced.proofs().len());
    assert!(matches!(Family::derive_native_v1(&b.state,frame,&source_owner,&retained,&lower),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(s)) if s.contains("producer provenance")));
    assert_eq!(family.len_v1(),u64::from(source_owner.vocab_v1()));assert_eq!(family.token_at_v1(0),Some(0));assert_eq!(family.token_at_v1(family.len_v1()-1),Some(source_owner.vocab_v1()-1));assert_eq!(family.token_at_v1(family.len_v1()),None);
    assert_eq!(family.context_requirement_v1(),TokenizerContextRequirementV1::ActualCurrentSignedIntentContext);
    let supply=family.unresolved_supply_v1();assert_eq!(supply.vocabulary,source_owner.vocab_v1());assert_eq!(supply.key_proofs,produced.proofs().len() as u64);assert_eq!(supply.raw_bytes,raw.len() as u64);
    // Exhaustive actual V fixture, no caller target subset. Same complete proof
    // sequence is independently replayed under every actual signed target context.
    for id in 0..source_owner.vocab_v1(){let mut target=intent.clone();target.token_id=id;target.material_id=family.material_id_v1(id,budget).unwrap();sign(&mut target);
        let checked=family.build_v1(&store,&b.state,&params,&rules,&target,budget).unwrap();
        let direct=store.build_tokenizer_terminal_retained_v1(&b.state,&params,&rules,&target,&retained,produced.proofs(),budget).unwrap();
        assert_eq!(checked.recheck(&store,&b.state,&params,&rules,budget).unwrap(),direct.recheck(&store,&b.state,&params,&rules,budget).unwrap());}
    assert!(family.material_id_v1(source_owner.vocab_v1(),budget).is_err());
    for limit in [required.get()-1,required.get()]{let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:limit,database_bytes:0});
        let got=Family::derive_reserved_v1(&b.state,frame,&source_owner,&retained,&produced,|q|checker.reserve_requests_v1(q,0).map(Some));
        if limit==required.get(){let held=got.unwrap();assert_eq!(checker.outstanding(),(required.get(),0));drop(held);}else{assert!(matches!(got,Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));}assert_eq!(checker.outstanding(),(0,0));}
    let mut wrong=intent.clone();wrong.inventory_id=h(0xDAB1);sign(&mut wrong);assert!(family.build_v1(&store,&b.state,&params,&rules,&wrong,budget).is_err());
    let mut wrong=intent.clone();wrong.token_id=source_owner.vocab_v1();sign(&mut wrong);assert!(family.build_v1(&store,&b.state,&params,&rules,&wrong,budget).is_err());
    let foreign=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1();let foreign_source=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(&foreign.record,&foreign.layout,foreign.frame.header_v1(),foreign.frame.original_v1().execution_root,foreign.frame.original_v1().trace_root,foreign.rules.maximum_leaves).unwrap();
    assert!(Family::derive_native_v1(&b.state,frame,&foreign_source,&retained,&produced).is_err());
    let mut replaced=b.state.clone();replaced.bonds.get_mut(&outsider()).unwrap().pubkey=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([99;32]).verification_key.as_ref().to_vec();assert!(family.build_v1(&store,&replaced,&params,&rules,&intent,budget).is_err());
}
