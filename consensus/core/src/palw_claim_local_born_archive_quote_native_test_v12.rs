{
// Candidate-only ordinary SIGNED TFO2 transition above has run both real
// observer hooks under the enclosing accepted object's actual journal spans.
// Original payment remains narrow; this DATA test creates no export-funded grant.
{
    use super::super::palw_claim_local_native_source_archive_cost_v10 as cost;
    use super::super::palw_claim_local_source_admission_v8 as admissions;
    use super::super::palw_claim_local_born_tir_source_v16 as originals;
    use crate::palw_tir_prefix_export_relation_v2::prepare_native_source_export_set_v2;
    let source=admissions::for_accepting_point_v8(&accepted,claim,ctx).unwrap().unwrap();
    assert_eq!(source.accepting_point(),ctx,"full ACTUAL fixture point including subsidy");
    let origin=originals::original_provenance_v16(&accepted,claim,ctx).unwrap().unwrap();
    assert!(origin.archive_quote_complete_v12());
    let frame=origin.born_postblock_frame();
    let native_source=frame.source_identity_native_v2(origin.archive_quote_born_class_v12(),origin.archive_quote_born_layout_v12(),frame.descriptor_v1().maximum_leaves).unwrap();
    let signed=&active.prefix_dependencies_v1[&claim].exports;
    let set=prepare_native_source_export_set_v2(&fp.commitment,&native_source,origin.archive_quote_born_class_v12(),origin.archive_quote_born_layout_v12(),&key,frame.descriptor_v1().maximum_leaves,signed).unwrap().unwrap();
    let finite=cost::ProspectiveArchiveHorizonV10{accept_from:ctx.daa_score,accept_until:ctx.daa_score,material_until:fp.commitment.trace_retention_daa};
    originals::assert_actual_archive_reverse_closure_v12(&accepted,&family_params,&active,claim,ctx);
    originals::assert_actual_marginal_dedup_v14(&accepted,&family_params,&active,claim,ctx);
    let before=accepted.state_root();
    let quote=cost::quote_native_source_archive_for_born_v12(&accepted,&source,&set,finite).unwrap();
    assert_eq!(quote.claim(),claim);assert_eq!(quote.accepting_point(),ctx);
    assert_eq!(quote.measured().source(),frame.plan_v1().source_id);
    assert_eq!(quote.measured().frame_bytes(),(borsh::object_length(origin.admitted_signed_frame()).unwrap() as u64,borsh::object_length(frame).unwrap() as u64));
    assert_eq!(quote.measured().source_body_rows(),8+u64::from(origin.archive_quote_semantics_v12().is_some()));
    assert!(quote.measured().durable_bytes_max()>quote.measured().frame_bytes().0);
    assert!(quote.measured().native_requests_max()>0);
    assert_eq!(quote.read_work().signed_export_identity_count,signed.len() as u64);
    assert!(quote.read_work().additional_native_reader_peak_missing,"full read/CPU quote remains explicitly missing");
    assert!(quote.measured().missing().future_rewrite_or_retention_events);
    assert!(cost::quote_native_source_archive_for_born_v12(&family_state,&source,&set,finite).is_err(),"actual source view cannot be lent to a state without that native record");
    let mut bad=finite;bad.material_until=ctx.daa_score;
    assert!(cost::quote_native_source_archive_for_born_v12(&accepted,&source,&set,bad).is_err());
    // Another genuinely signed and natively prepared export is STILL not the
    // original accepted population. No producer-supplied byte count substitutes.
    let mut other=signed.clone();other[0].intent.ordinal=other[0].intent.ordinal.checked_add(1).unwrap();
    other[0].signature=mldsa::sign(&kp.signing_key,other[0].intent.id_v1().as_byte_slice(),crate::palw_claim_local_v1::prefix_v1::PREFIX_EXPORT_CONTEXT_V1,seed).unwrap().as_ref().to_vec();
    let other_set=prepare_native_source_export_set_v2(&fp.commitment,&native_source,origin.archive_quote_born_class_v12(),origin.archive_quote_born_layout_v12(),&key,frame.descriptor_v1().maximum_leaves,&other).unwrap().unwrap();
    assert!(cost::quote_native_source_archive_for_born_v12(&accepted,&source,&other_set,finite).is_err(),"valid signature on a different intent is not archived-original identity");
    assert_eq!(accepted.state_root(),before,"readonly quotes never change state/journal");
    let loaded=PalwStateCarriageV2::from_state(&accepted).into_state(&family_params,Some(before)).unwrap();
    let cold=admissions::for_accepting_point_v8(&loaded,claim,ctx).unwrap().unwrap();
    let cold_quote=cost::quote_native_source_archive_for_born_v12(&loaded,&cold,&set,finite).unwrap();
    assert_eq!(cold_quote.source_record(),quote.source_record());
    assert_eq!(cold_quote.original_bundle(),quote.original_bundle());
    assert_eq!(cold_quote.measured().durable_bytes_max(),quote.measured().durable_bytes_max());
    let mut reverted=accepted.clone();
    for entry in accepted_delta.entries.iter().rev(){apply_delta_entry(&mut reverted,entry,true).unwrap();}
    assert_eq!(reverted.state_root(),family_state.state_root(),"actual undo removes source DATA and ordinary money effects together");
    assert!(originals::original_provenance_v16(&reverted,claim,ctx).unwrap().is_none());
    for entry in &accepted_delta.entries {apply_delta_entry(&mut reverted,entry,false).unwrap();}
    assert_eq!(reverted.state_root(),before);
    let replay=admissions::for_accepting_point_v8(&reverted,claim,ctx).unwrap().unwrap();
    assert_eq!(cost::quote_native_source_archive_for_born_v12(&reverted,&replay,&set,finite).unwrap().original_bundle(),quote.original_bundle());
}

// Genuine signed TFO2 admission above, plus native journal/reorg/cold source
// reads. Source16 is test-only DATA; no funded export/Open or credit is granted.
{
    use super::super::palw_claim_local_born_tir_source_v16 as originals;
    use super::super::palw_claim_local_prefix_tir_context_v2 as born_context;
    use super::super::palw_claim_local_rewards_v1 as rewards;
    use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 as TABLE;
    use crate::palw_native_resource_v1::{TirNativeRequestsV1,reserve_native_requests_v1};
    let archived=originals::original_provenance_v16(&accepted,claim,ctx).unwrap().unwrap();
    let ordinal=active.prefix_dependencies_v1[&claim].exports[0].intent.ordinal;
    let (export,identity)=archived.prefix_export_v1(ordinal).unwrap().unwrap();
    assert_eq!(export,&active.prefix_dependencies_v1[&claim].exports[0]);
    let reservation=accepted.prefix_export_reservation_v1(&ctx,claim).unwrap().unwrap();
    assert_eq!(reservation.point,ctx);assert_eq!(reservation.source_claim,claim);
    let expected_program=crate::palw_native_resource_v1::decode_native_tir_program_v1(
        archived.archive_quote_born_class_v12().program.as_slice()).unwrap();
    let read=|state:&PalwChainStateV2|born_context::with_born_export_v2(state,&family_params,&identity,ordinal,&reservation,|born|{
        assert_eq!(born.target().source,identity);assert_eq!(born.target().prefix,export.intent.prefix);
        assert_eq!(born.target().source_execution_root,fp.commitment.execution_root);
        assert_eq!(born.spec().intent_id,identity.intent);assert_eq!(born.spec().export_ordinal,ordinal);
        assert_eq!(born.spec().source_id,archived.born_postblock_frame().plan_v1().source_id);
        assert_eq!(born.original(),archived.original());assert_eq!(born.frame(),archived.born_postblock_frame());
        assert_eq!(born.reservation(),&reservation);assert_eq!(born.source().header_v1(),born.frame().header_v1());
        assert_eq!(&born.space().program,&*expected_program);
        assert_eq!(&born.space().layout,archived.archive_quote_born_layout_v12());
        let fingerprint=crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka/test/exact-born-tir-view/v13",&(born.frame(),&born.space().program,&born.space().layout))
            .map_err(|_|PalwStateV2Error::DeltaMismatch("native born view digest"))?;
        Ok((born.target(),born.spec(),born.source().source_id_v1(),fingerprint))
    });
    let expected=read(&accepted).unwrap();
    // Genuine compatibility candidate: the SAME signed object/payer/suppliers
    // and native fold, with only the private DATA observer defaulted false.
    let mut legacy_extras=active.clone();legacy_extras.original_archive_data_observer_v16=false;
    let (legacy,_)=apply_palw_transition_v2_with_extras(&family_state,&family_params,&ctx,
        std::slice::from_ref(&object),None,false,false,false,false,&legacy_extras).unwrap();
    assert!(originals::original_provenance_v16(&legacy,claim,ctx).unwrap().is_none());
    assert_eq!(read(&legacy).unwrap(),expected,"absent Source16 uses authentic complete legacy F1");
    assert_eq!(legacy.claim(&claim),accepted.claim(&claim));
    assert_eq!(rewards::cert_bytes(legacy.kernel_route_state_v1(),claim),rewards::cert_bytes(accepted.kernel_route_state_v1(),claim),
        "data archives do not manufacture selected verifier approval");
    let ordinary_status=rewards::palw_claim_local_recorded_settlement_v1(&legacy,&family_params,claim,legacy.claim(&claim).unwrap(),ctx.daa_score).unwrap();
    let archived_status=rewards::palw_claim_local_recorded_settlement_v1(&accepted,&family_params,claim,accepted.claim(&claim).unwrap(),ctx.daa_score).unwrap();
    assert_eq!(ordinary_status,archived_status,"same native source readiness before/after DATA capture");
    // Exact-source callback errors remain the caller's error, not a successful
    // fallback or approval. The source view cannot be consumed a second time.
    let error=born_context::with_born_export_v2::<()>(&accepted,&family_params,&identity,ordinal,&reservation,|_|Err(PalwStateV2Error::DeltaMismatch("exact scoped callback refusal"))).unwrap_err();
    assert!(matches!(error,PalwStateV2Error::DeltaMismatch("exact scoped callback refusal")));
    // Real complete native delta replay/reorg, never re-injected Claim/Frame.
    let replay=apply_delta_v2(&family_state,&accepted_delta,&family_params).unwrap();
    assert_eq!(replay.state_root(),accepted.state_root());assert_eq!(read(&replay).unwrap(),expected);
    let inverse=revert_delta_v2(&accepted,&accepted_delta,&family_params).unwrap();
    assert_eq!(inverse.state_root(),family_state.state_root());
    assert!(born_context::with_born_export_v2(&inverse,&family_params,&identity,ordinal,&reservation,|_|Ok(())).is_err());
    let replay_again=apply_delta_v2(&inverse,&accepted_delta,&family_params).unwrap();assert_eq!(read(&replay_again).unwrap(),expected);
    // Diagnostic F0 ONLY: delete real native F rows through their journaled
    // writer, bind the resulting complete native root, and cold-restore it.
    // This does not claim a production GC/financial permission to delete F.
    let frame_count=rewards::frames(&accepted).count();let mut q=TirNativeRequestsV1::new();
    q.exact_v1::<[u8;65]>(frame_count as u64).unwrap();let _keys=reserve_native_requests_v1(q.bytes_v1()).unwrap();
    let mut keys=Vec::<[u8;65]>::with_capacity(frame_count);
    for(key,_)in rewards::frames(&accepted){keys.push(key.as_slice().try_into().unwrap());}
    assert!(!keys.is_empty());
    let mut diagnostic=TransitionBuilder::try_new(&accepted,&family_params,false,false,false,false,&active).unwrap();
    diagnostic.claim_local_lock_point_v6=Some(ctx);
    for key in &keys {diagnostic.write_kernel_row_encoded_v1::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(TABLE,key,None).unwrap();}
    assert_eq!(rewards::frames(&diagnostic.state).count(),0);assert_eq!(read(&diagnostic.state).unwrap(),expected);
    let f_delta=PalwStateDeltaV2{point:ctx,entries:diagnostic.entries.clone()};let f_root=diagnostic.state.state_root();
    let _carriage=reserve_native_requests_v1(crate::palw_native_resource_v1::native_value_requests_v1(&diagnostic.state).unwrap()).unwrap();
    let cold_f0=PalwStateCarriageV2::from_state(&diagnostic.state).into_state(&family_params,Some(f_root)).unwrap();
    assert_eq!(cold_f0.state_root(),f_root);assert_eq!(rewards::frames(&cold_f0).count(),0);assert_eq!(read(&cold_f0).unwrap(),expected);
    assert_eq!(cold_f0.claim(&claim),accepted.claim(&claim));
    assert_eq!(rewards::cert_bytes(cold_f0.kernel_route_state_v1(),claim),rewards::cert_bytes(accepted.kernel_route_state_v1(),claim));
    assert_eq!(revert_delta_v2(&cold_f0,&f_delta,&family_params).unwrap().state_root(),accepted.state_root());
    assert_eq!(apply_delta_v2(&accepted,&f_delta,&family_params).unwrap().state_root(),f_root);
    // Present Source16 corrupt/missing manifest must fail even while authentic
    // F1 is still available; it cannot masquerade as an absent legacy source.
    {
        use super::super::palw_claim_local_context_store_v2::StateNodesV2;
        use crate::palw_claim_local_context_nodes_v2::{self as nodes,ComponentSpecV2};
        let reader=StateNodesV2(&accepted);let directory=reader.current(ComponentSpecV2{owner:16,table:1,schema:10,key_bytes:64,value_bytes:u64::MAX}).unwrap();
        assert_eq!(directory.rows,1);let mut manifest=None;
        nodes::complete_rows(&reader,directory,|_,rv|{assert!(manifest.replace(rv).is_none());Ok(())}).unwrap();
        let mut key=[0;65];key[0]=b'B';key[1..].copy_from_slice(manifest.unwrap().data.as_byte_slice());
        let mut corrupt=TransitionBuilder::try_new(&accepted,&family_params,false,false,false,false,&active).unwrap();
        corrupt.write_kernel_row_encoded_v1::<Vec<u8>>(TABLE,&key,None).unwrap();
        assert!(rewards::frames(&corrupt.state).count()>0);
        let called=std::cell::Cell::new(false);
        assert!(born_context::with_born_export_v2(&corrupt.state,&family_params,&identity,ordinal,&reservation,|_|{called.set(true);Ok(())}).is_err());
        assert!(!called.get(),"corrupt present scope cannot invoke legacy callback");
    }
    // No caller may relabel old authentic bytes as a future/new birth, another
    // ordinal, or a reservation with a fabricated full actual context.
    {
        let mut future=identity;future.accepted_daa=future.accepted_daa.checked_add(1).unwrap();future.accepted_blue_score=future.accepted_blue_score.checked_add(1).unwrap();
        let called=std::cell::Cell::new(false);
        assert!(born_context::with_born_export_v2(&cold_f0,&family_params,&future,ordinal,&reservation,|_|{called.set(true);Ok(())}).is_err());assert!(!called.get());
        assert!(born_context::with_born_export_v2(&cold_f0,&family_params,&identity,ordinal.checked_add(1).unwrap(),&reservation,|_|Ok(())).is_err());
        let mut wrong=reservation.clone();wrong.point.subsidy=wrong.point.subsidy.checked_add(1).unwrap();
        assert!(born_context::with_born_export_v2(&cold_f0,&family_params,&identity,ordinal,&wrong,|_|Ok(())).is_err());
    }
    // Ordinary signed TFO2 re-admission cannot coexist with this native frame.
    // original_plan checks Claim OR TirClaim before allocation. Source16's
    // deletion hook releases its data when that exact old frame is removed;
    // forcing an overlapping second paid birth would require fake state edits.
    {
        let readmit=TransitionBuilder::try_new(&accepted,&family_params,false,false,false,false,&active).unwrap();
        let blocked=original_plan_v1(&readmit,ctx.daa_score.checked_add(1).unwrap(),&fp.commitment,archived.born_postblock_frame().header_v1());
        assert!(blocked.is_err());assert!(blocked.err().unwrap().to_string().contains("existing claim"));
        let mut next=ctx;next.daa_score=next.daa_score.checked_add(1).unwrap();next.blue_score=next.blue_score.checked_add(1).unwrap();next.block=h(0xF136);
        assert!(apply_palw_transition_v2_with_extras(&accepted,&family_params,&next,std::slice::from_ref(&object),None,false,false,false,false,&active).is_err());
        assert_eq!(read(&accepted).unwrap(),expected,"a rejected new paid Original does not replace the old born view");
    }
    // Diagnostic visibility clock only: all Claim/Frame/archive bytes remain
    // genuine, but the local accepted tip has not published this birth yet.
    {
        let mut prior=ctx;prior.daa_score=prior.daa_score.saturating_sub(1);prior.blue_score=prior.blue_score.checked_sub(1).unwrap();prior.block=h(0xF134);
        let mut unpublished=accepted.clone();unpublished.last_point=Some(prior);
        let mut legacy_unpublished=legacy.clone();legacy_unpublished.last_point=Some(prior);
        let called=std::cell::Cell::new(false);
        let a=born_context::with_born_export_v2(&unpublished,&family_params,&identity,ordinal,&reservation,|_|{called.set(true);Ok(())}).unwrap_err();
        let b=born_context::with_born_export_v2(&legacy_unpublished,&family_params,&identity,ordinal,&reservation,|_|{called.set(true);Ok(())}).unwrap_err();
        assert!(!called.get());assert!(matches!(a,PalwStateV2Error::NativeContextUnavailable(_)));assert_eq!(std::mem::discriminant(&a),std::mem::discriminant(&b));assert_eq!(a.to_string(),b.to_string());
        unpublished.last_point=None;
        assert!(matches!(born_context::with_born_export_v2(&unpublished,&family_params,&identity,ordinal,&reservation,|_|{called.set(true);Ok(())}),Err(PalwStateV2Error::NativeContextUnavailable(_))));assert!(!called.get());
    }
    // Disabled fork uses the exact legacy outcome/error class despite a present
    // Source16 scope; the private DATA observer cannot confer dormant authority.
    {
        let off=family_params.clone().with_claim_local_settlement_from_daa(None);
        let a=born_context::with_born_export_v2(&accepted,&off,&identity,ordinal,&reservation,|born|Ok(born.spec()));
        let b=born_context::with_born_export_v2(&legacy,&off,&identity,ordinal,&reservation,|born|Ok(born.spec()));
        match (a,b){(Ok(a),Ok(b))=>assert_eq!(a,b),(Err(a),Err(b))=>{assert_eq!(std::mem::discriminant(&a),std::mem::discriminant(&b));assert_eq!(a.to_string(),b.to_string());},_=>panic!("Source16 data changes dormant legacy outcome")}
        let called=std::cell::Cell::new(false);
        assert!(born_context::with_born_export_v2(&cold_f0,&off,&identity,ordinal,&reservation,|_|{called.set(true);Ok(())}).is_err());assert!(!called.get());
    }
    // A genuine legacy source without either scoped Source16 or F1 remains
    // unavailable. Missing history never becomes an invented partial world.
    {
        let mut unavailable=TransitionBuilder::try_new(&legacy,&family_params,false,false,false,false,&legacy_extras).unwrap();
        for key in &keys{unavailable.write_kernel_row_encoded_v1::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(TABLE,key,None).unwrap();}
        assert_eq!(rewards::frames(&unavailable.state).count(),0);
        let called=std::cell::Cell::new(false);
        assert!(born_context::with_born_export_v2(&unavailable.state,&family_params,&identity,ordinal,&reservation,|_|{called.set(true);Ok(())}).is_err());assert!(!called.get());
    }

}

}
