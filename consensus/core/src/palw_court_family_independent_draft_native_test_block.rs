{
// Independent optional uploader: a real native body, signed exact amounts,
// actual new reservations, two concurrent births and cold/reorg retention.
let c=&consents[0];let inventory=&inventories[0];
let target=inventory.targets_v1().iter().map(|(t,_)|*t).find(|t|matches!(t,fm::FamilyMaterialTargetV1::Input(MT::PromptFlat|MT::Parameter{..}))).unwrap();
let body=match target{fm::FamilyMaterialTargetV1::Input(MT::PromptFlat)=>borsh::to_vec(&fx.execution.prompt).unwrap(),fm::FamilyMaterialTargetV1::Input(MT::Parameter{leaf})=>borsh::to_vec(&fx.execution.param_opening(leaf).unwrap()).unwrap(),_=>unreachable!()};
let contract=c.id_v1().unwrap();let uploader=outsider();
// Primary material and existing holding packets legitimately own early slots.
// Select a new birth from the ACTUAL residual native book, never by raising TTL
// or moving any old promise. Signing still uses the existing acceptance interval.
let native_span=crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1.max(inventory.native_inclusion_margin_v2());
let transfers=body.len().div_ceil(fm::FAMILY_MATERIAL_PART_BYTES_V1) as u64+1;
let ttl=crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().unwrap().draft_ttl_daa;
let duration=transfers.checked_mul(native_span).unwrap();
assert!(duration.checked_add(native_span).unwrap()<=ttl);
let transfer_cost=crate::palw_court_service_v1::ServiceCost{
 bytes:crate::palw_g14_values_v1::PALW_G14_SOURCE_PACKET_COST_V1.bytes.max((fm::FAMILY_MATERIAL_PART_BYTES_V1+crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN+512)as u64),
 work:crate::palw_g14_values_v1::PALW_G14_SOURCE_PACKET_COST_V1.work,io:crate::palw_g14_values_v1::PALW_G14_SOURCE_PACKET_COST_V1.io,controls:1};
let costs=vec![transfer_cost;usize::try_from(transfers).unwrap()];
let sign=|q:fm::FamilyDraftQuoteV2|fm::sign_family_material_v1(fm::SignedFamilyMaterialV1{version:2,contract,target,submitter:uploader,action:fm::FamilyMaterialActionV1::BeginReserved{total_bytes:body.len() as u64,body_hash:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body),quote:q},signature:vec![]},network,|_,msg,context|Some(mldsa::sign(&family_filer_key.signing_key,msg,context,[0x76;32]).unwrap().as_ref().to_vec())).unwrap();
let next_birth=|state:&PalwChainStateV2,minimum:u64,quote_before:bool|{
 let first=minimum.checked_add(native_span).unwrap();
 let last=c.terms.respond_until_daa.checked_sub(duration).unwrap();
 let book=state.court_service_state_v1().unwrap();
 let start=super::super::palw_court_family_supplier_fold_v1::calendar_search::first_fit_width_v2(&book.book,&costs,first,last,native_span,1).unwrap().expect("an independent uploader needs a genuine unreserved native chain before original hard");
 let birth=start.checked_sub(native_span).unwrap();
 let at=PalwBlockContextV2{daa_score:if quote_before{birth-1}else{birth},blue_score:state.last_point().unwrap().blue_score+1,block:h(0x449020),subsidy:0};
 let q=super::super::palw_court_family_material_fold_v1::palw_family_material_begin_quote_v2(state,&family_params,&active,&at,contract,target,uploader,body.len()as u64).unwrap();
 let legacy=super::super::palw_court_family_material_fold_v1::draft_reservations::legacy_quote_diagnostic_v3(state,&family_params,&active,&at,contract,target,uploader,body.len()as u64).unwrap();
 let row_for=|quote:fm::FamilyDraftQuoteV2|{
  let begin=sign(quote.clone());let draft=fm::family_material_draft_id_v2(contract,target,uploader,body.len()as u64,crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body),&quote).unwrap();
  fm::FamilyDraftReservationV2{contract,version:2,claim:c.terms.claim,draft,accepted_daa:birth,expires_daa:(birth+ttl).min(c.terms.respond_until_daa),material_until_daa:c.terms.material_until_daa,quote,transfer_first_daa:start,transfer_span_daa:native_span,begin}
 };
 // Read-only actual complete plan: transfers AND all own holding packets.
 // These signed DTOs install nothing and confer no native paid standing.
 let planned=row_for(q.clone());let complete=super::super::palw_court_family_material_fold_v1::draft_reservations::packets(&planned).unwrap();
 let old=row_for(legacy.clone());let old_complete=super::super::palw_court_family_material_fold_v1::draft_reservations::packets(&old).unwrap();
 let native=crate::palw_native_resource_v1::native_value_requests_v1(book).unwrap();let _local=crate::palw_native_resource_v1::reserve_native_requests_v1(native.checked_mul(2).unwrap().checked_add(q.reserved_memory.max(legacy.reserved_memory)).unwrap()).unwrap();
 let old_result={let mut probe=book.book.clone();probe.reserve(&old_complete,c.terms.respond_until_daa)};
 let result={let mut probe=book.book.clone();probe.reserve(&complete,c.terms.respond_until_daa)};
 eprintln!("[independent-draft-complete-quote] birth={} bookPackets={} bookHeap={} bookBytes={} localWorkspace={} oldHolding={} ownHolding={} transfers={} oldReserve={:?} ownReserve={:?}",birth,book.book.packets.len(),q.book_heap,q.book_bytes,q.book_heap.checked_mul(4).unwrap()+q.book_bytes.checked_mul(2).unwrap(),legacy.reserved_memory.div_ceil(crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes),q.reserved_memory.div_ceil(crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes),transfers,old_result,result);
 result.expect("actual complete transfer+ownholding plan must fit unchanged native book/hard/TTL");
 assert!(q.reserved_memory<legacy.reserved_memory,"old wholebook transient workspace is retained locally, not charged as new durable holding");
 (birth,q)
};
// One real observation separates the quote from acceptance, as before. Reserve
// that separation when choosing the first actual transfer chain.
let early=ctx.daa_score.checked_add(2).unwrap();
let (begin_at,planned_quote)=next_birth(&accepted,early,true);
let point=PalwBlockContextV2{daa_score:begin_at.checked_sub(1).unwrap(),blue_score:ctx.blue_score+1,block:h(0x449001),subsidy:0};
let quote=super::super::palw_court_family_material_fold_v1::palw_family_material_begin_quote_v2(&accepted,&family_params,&active,&point,contract,target,uploader,body.len() as u64).unwrap();
assert_eq!(quote,planned_quote,"full-calendar probe used the same actual quote before signing");
assert!(quote.fee_sompi>0&&quote.collateral_sompi>0);
let control=sign(quote.clone());
if begin_at>early {
 let early_quote_point=PalwBlockContextV2{daa_score:early-1,..point};
 let early_quote=super::super::palw_court_family_material_fold_v1::palw_family_material_begin_quote_v2(&accepted,&family_params,&active,&early_quote_point,contract,target,uploader,body.len()as u64).unwrap();
 let early_control=sign(early_quote);let at=PalwBlockContextV2{daa_score:early,..point};
 let mut rejected=TransitionBuilder::try_new(&accepted,&family_params,false,false,false,false,&active).unwrap();rejected.claim_local_lock_point_v6=Some(at);
 let before=rejected.state.state_root();let journal=borsh::to_vec(&rejected.entries).unwrap();
 let error=super::super::palw_court_family_material_fold_v1::apply_family_material_v1(&mut rejected,&at,&early_control).unwrap_err();
 assert!(matches!(error,PalwStateV2Error::CarriageInconsistent(ref why)if why=="independent multipart causal chain cannot fit native book/TTL"),"earliest native slots remain reserved: {error:?}");
 assert_eq!(rejected.state.state_root(),before);assert_eq!(borsh::to_vec(&rejected.entries).unwrap(),journal);
}
eprintln!("[independent-draft-native-calendar] earliestBirth={} selectedBirth={} span={} TTL={} hard={} existingPackets={}",early,begin_at,native_span,ttl,c.terms.respond_until_daa,accepted.court_service_state_v1().unwrap().book.packets.len());
for insufficient in [false,true]{
 let mut prior=accepted.clone();
 if insufficient{let mut read=TransitionBuilder::try_new(&prior,&family_params,false,false,false,false,&active).unwrap();let reserved=super::super::palw_court_capacity_fold_v1::fee_reserved_principal_v1(&read,&uploader,point.daa_score).unwrap();let keep=u64::try_from(reserved).unwrap()+quote.fee_sompi+quote.collateral_sompi-1;let amount=read.state.bond(&uploader).unwrap().collateral.checked_sub(keep).unwrap();read.slash_bond(uploader,u128::from(amount)).unwrap();prior=read.state.clone();}
 let mut b=TransitionBuilder::try_new(&prior,&family_params,false,false,false,false,&active).unwrap();let before=b.state.state_root();let journal=borsh::to_vec(&b.entries).unwrap();
 let mut candidate=control.clone();if !insufficient{let mut stale=quote.clone();stale.fee_sompi-=1;candidate=sign(stale);}
 assert!(super::super::palw_court_family_material_fold_v1::apply_family_material_v1(&mut b,&point,&candidate).is_err());
 assert_eq!(b.state.state_root(),before);assert_eq!(borsh::to_vec(&b.entries).unwrap(),journal);
}
let object=PalwConsensusObjectV2::CourtFamilyMaterialV1{control:Box::new(control.clone())};
// The quote is signed before an intervening real observation. No unknown
// accepting block hash, supply history, or exact future acceptance DAA was signed.
let wait_point=PalwBlockContextV2{block:h(0x449010),..point};
let(warmed,_)=apply_palw_transition_v2_with_extras(&accepted,&family_params,&wait_point,&[],None,false,false,false,false,&active).unwrap();
assert_ne!(borsh::to_vec(accepted.court_service_state_v1().unwrap()).unwrap(),borsh::to_vec(warmed.court_service_state_v1().unwrap()).unwrap());
let point=PalwBlockContextV2{daa_score:point.daa_score+1,blue_score:point.blue_score+1,block:h(0x449011),subsidy:0};
{
 // The real intervening observation/recovery may change assignments. Recheck
 // the SAME signed quote's complete plan against this actual warmed prefix.
 let old=warmed.court_service_state_v1().unwrap();let expiry=(point.daa_score+ttl).min(c.terms.respond_until_daa);
 let first=super::super::palw_court_family_supplier_fold_v1::calendar_search::first_fit_width_v2(&old.book,&costs,point.daa_score+native_span,expiry-duration,native_span,1).unwrap().expect("actual warmed prefix must still admit the signed transfer chain");
 let draft=fm::family_material_draft_id_v2(contract,target,uploader,body.len()as u64,crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body),&quote).unwrap();
 let planned=fm::FamilyDraftReservationV2{contract,version:2,claim:c.terms.claim,draft,accepted_daa:point.daa_score,expires_daa:expiry,material_until_daa:c.terms.material_until_daa,quote:quote.clone(),transfer_first_daa:first,transfer_span_daa:native_span,begin:control.clone()};
 let complete=super::super::palw_court_family_material_fold_v1::draft_reservations::packets(&planned).unwrap();
 let native=crate::palw_native_resource_v1::native_value_requests_v1(old).unwrap();let _local=crate::palw_native_resource_v1::reserve_native_requests_v1(native.checked_mul(2).unwrap().checked_add(quote.reserved_memory).unwrap()).unwrap();
 let result={let mut candidate=old.book.clone();candidate.reserve(&complete,c.terms.respond_until_daa)};
 eprintln!("[independent-draft-warmed-complete-calendar] birth={} first={} holding={} packets={} reserve={:?}",point.daa_score,first,quote.reserved_memory.div_ceil(crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes),complete.len(),result);
 result.expect("real observation cannot be skipped when proving complete native calendar capacity");
}
let (begun,delta)=apply_palw_transition_v2_with_extras(&warmed,&family_params,&point,std::slice::from_ref(&object),None,false,false,false,false,&active).unwrap();
let digest=crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
let id=fm::family_material_draft_id_v2(contract,target,uploader,body.len() as u64,digest,&quote).unwrap();
let receipt_key=(fm::FAMILY_MATERIAL_TABLE_V1,fm::material_aux_key_v1(5,id,None));
assert!(begun.kernel_route_state_v1().unwrap().aux.contains_key(&receipt_key),"the genuine signed optional draft is accepted");
let born:fm::FamilyDraftReservationV2=borsh::from_slice(&begun.kernel_route_state_v1().unwrap().aux[&receipt_key]).unwrap();
let scheduled=super::super::palw_court_family_material_fold_v1::draft_reservations::packets(&born).unwrap();
let transfer_count=body.len().div_ceil(fm::FAMILY_MATERIAL_PART_BYTES_V1)+1;
for i in 0..transfer_count {
 assert!(scheduled[i].release_daa>born.accepted_daa);
 assert!(scheduled[i].service_before<=born.expires_daa,"every transfer fits the existing native draft TTL");
 if i>0{assert!(scheduled[i-1].service_before<=scheduled[i].release_daa,"part and Finish cannot precede their causal predecessor");}
 assert_eq!(begun.court_service_state_v1().unwrap().book.packet(&scheduled[i].id),Some(&scheduled[i]));
}

// Execute the independently signed body at its actual packet opportunities.
// This is an isolated positive branch; the concurrent/expiry branch below starts
// from the same genuine Begin and does not borrow this branch's supply.
let mut delivered=begun.clone();let mut tick=point.blue_score;
for (ordinal,action) in body.chunks(fm::FAMILY_MATERIAL_PART_BYTES_V1).enumerate().map(|(i,bytes)|(i,fm::FamilyMaterialActionV1::Part{draft:id,offset:(i*fm::FAMILY_MATERIAL_PART_BYTES_V1) as u64,bytes:bytes.to_vec()})).chain(std::iter::once((transfer_count-1,fm::FamilyMaterialActionV1::Finish{draft:id}))) {
 let assigned=delivered.court_service_state_v1().unwrap().book.assigned_daa(&scheduled[ordinal].id).unwrap();
 let mut daa=delivered.last_point().unwrap().daa_score;assert!(assigned>=daa+born.transfer_span_daa);
 while daa+1<assigned {daa+=1;tick+=1;let beat=PalwBlockContextV2{daa_score:daa,blue_score:tick,block:h(0x44A000+tick),subsidy:0};delivered=apply_palw_transition_v2_with_extras(&delivered,&family_params,&beat,&[],None,false,false,false,false,&active).unwrap().0;}
 tick+=1;let at=PalwBlockContextV2{daa_score:assigned,blue_score:tick,block:h(0x44A000+tick),subsidy:0};
 let signed=fm::sign_family_material_v1(fm::SignedFamilyMaterialV1{version:2,contract,target,submitter:uploader,action,signature:vec![]},network,|_,msg,context|Some(mldsa::sign(&family_filer_key.signing_key,msg,context,[0x76;32]).unwrap().as_ref().to_vec())).unwrap();
 let op=PalwConsensusObjectV2::CourtFamilyMaterialV1{control:Box::new(signed)};
 assert!(crate::palw_court_transport_v1::family_material_accounted_v2(&delivered,&family_params,assigned,&op).unwrap().unwrap().2,"extra draft transfer must match exact native service opportunity");
 delivered=apply_palw_transition_v2_with_extras(&delivered,&family_params,&at,&[op],None,false,false,false,false,&active).unwrap().0;
}
assert_eq!(fm::read_authenticated_family_material_classified_v1(&delivered,inventory,target).unwrap().unwrap().bytes,body);
let wire=borsh::to_vec(&PalwStateCarriageV2::from_state(&delivered)).unwrap();assert_eq!(borsh::from_slice::<PalwStateCarriageV2>(&wire).unwrap().into_state(&family_params,Some(delivered.state_root())).unwrap().state_root(),delivered.state_root());
assert_eq!(begun.bond(&uploader).unwrap().collateral,accepted.bond(&uploader).unwrap().collateral-quote.fee_sompi);
let read=TransitionBuilder::try_new(&begun,&family_params,false,false,false,false,&active).unwrap();let baseline=TransitionBuilder::try_new(&accepted,&family_params,false,false,false,false,&active).unwrap();
assert_eq!(read.committed_at(&uploader,point.daa_score).unwrap(),baseline.committed_at(&uploader,point.daa_score).unwrap()+u128::from(quote.collateral_sompi));drop(read);drop(baseline);
let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&begun)).unwrap();let restored=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&family_params,Some(begun.state_root())).unwrap();assert_eq!(restored.state_root(),begun.state_root());
let mut undone=begun.clone();for entry in delta.entries.iter().rev(){apply_delta_entry(&mut undone,entry,true).unwrap();}undone.set_last_point_for_tests(warmed.last_point().cloned());assert_eq!(undone.state_root(),warmed.state_root());
// A second simultaneous body at a new quoted point pays for its own geometry
// AND the enlarged Q/M index over every previously bounded control lifecycle.
let (second_at,planned_second_quote)=next_birth(&begun,point.daa_score,false);
assert!(second_at<born.expires_daa,"two independently funded births really coexist within the original TTL");
let second_point=PalwBlockContextV2{daa_score:second_at,block:h(0x449002),blue_score:point.blue_score+1,..point};
let second_quote=super::super::palw_court_family_material_fold_v1::palw_family_material_begin_quote_v2(&begun,&family_params,&active,&second_point,contract,target,uploader,body.len() as u64).unwrap();
assert_eq!(second_quote,planned_second_quote);
assert!(second_quote.prior_events>=quote.prior_events);assert!(second_quote.fee_sompi>quote.fee_sompi);
let second_id=fm::family_material_draft_id_v2(contract,target,uploader,body.len() as u64,digest,&second_quote).unwrap();assert_ne!(id,second_id,"same literal data at a new signed quote is a distinct birth");
let (two,_)=apply_palw_transition_v2_with_extras(&begun,&family_params,&second_point,&[PalwConsensusObjectV2::CourtFamilyMaterialV1{control:Box::new(sign(second_quote.clone()))}],None,false,false,false,false,&active).unwrap();
assert!(two.kernel_route_state_v1().unwrap().aux.contains_key(&(fm::FAMILY_MATERIAL_TABLE_V1,fm::material_aux_key_v1(5,second_id,None))));
assert!(two.kernel_route_state_v1().unwrap().aux.contains_key(&(fm::FAMILY_MATERIAL_TABLE_V1,fm::material_aux_key_v1(1,id,None))),"the first real draft is still active when the second birth is admitted");
// Reuse after expiry cannot collide with either immutable past birth.
let second_born:fm::FamilyDraftReservationV2=borsh::from_slice(&two.kernel_route_state_v1().unwrap().aux[&(fm::FAMILY_MATERIAL_TABLE_V1,fm::material_aux_key_v1(5,second_id,None))]).unwrap();
let later=PalwBlockContextV2{daa_score:born.expires_daa.max(second_born.expires_daa).checked_add(1).unwrap(),blue_score:second_point.blue_score+1,block:h(0x449003),subsidy:0};
let(expired,_)=apply_palw_transition_v2_with_extras(&two,&family_params,&later,&[],None,false,false,false,false,&active).unwrap();
assert!(expired.kernel_route_state_v1().unwrap().aux.contains_key(&receipt_key));
assert!(!expired.kernel_route_state_v1().unwrap().aux.contains_key(&(fm::FAMILY_MATERIAL_TABLE_V1,fm::material_aux_key_v1(1,id,None))));
let (retry_at,planned_retry_quote)=next_birth(&expired,later.daa_score,false);
let retry_point=PalwBlockContextV2{daa_score:retry_at,block:h(0x449004),blue_score:later.blue_score+1,..later};
let retry_quote=super::super::palw_court_family_material_fold_v1::palw_family_material_begin_quote_v2(&expired,&family_params,&active,&retry_point,contract,target,uploader,body.len() as u64).unwrap();
assert_eq!(retry_quote,planned_retry_quote);
let retry_id=fm::family_material_draft_id_v2(contract,target,uploader,body.len() as u64,digest,&retry_quote).unwrap();assert_ne!(retry_id,id);assert_ne!(retry_id,second_id);
let(retry,_)=apply_palw_transition_v2_with_extras(&expired,&family_params,&retry_point,&[PalwConsensusObjectV2::CourtFamilyMaterialV1{control:Box::new(sign(retry_quote))}],None,false,false,false,false,&active).unwrap();
assert!(retry.kernel_route_state_v1().unwrap().aux.contains_key(&(fm::FAMILY_MATERIAL_TABLE_V1,fm::material_aux_key_v1(1,retry_id,None))),"a legal same-body retry is not permanently refused by old ID collision");
// Physical packing uses one real control per existing service slot.
let mut charge=TransitionBuilder::try_new(&accepted,&family_params,false,false,false,false,&active).unwrap();
super::super::palw_court_capacity_fold_v1::charge_court_op_v1(&mut charge,&point,&object).unwrap();assert!(super::super::palw_court_capacity_fold_v1::charge_court_op_v1(&mut charge,&point,&object).is_err());
}
