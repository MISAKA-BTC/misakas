// Included after ONE ordinary, really signed native Source17 full fold.
{
 use super::super::palw_claim_local_born_tir_source_v16 as archive;
 let context=super::super::palw_native_original_input_context_v19(&family_state,&ctx,
  std::iter::once(&object),PalwBlockWorkV3::None,&[],Hash64::default()).unwrap();
 let original_root=family_state.state_root();
 assert!(super::super::palw_v2_apply_one_object_retained_v1(&family_state,&family_params,&ctx,&object,false,false,false,false,&active).is_err(),"recognized profile cannot fabricate a missing actual input basis");
 let partial=super::super::palw_v2_apply_one_object_with_native_original_input_context_v19(&family_state,&family_params,&ctx,&object,false,false,false,false,&active,&context).unwrap();
 assert!(partial.claim(&claim).is_some());
 let pending=finance::verified_original_archive_reservation_v1(&partial,claim,ctx).unwrap().unwrap();
 assert!(!pending.constructed_v1(),"singleton filtering is not completed source/payment authority");
 assert!(super::super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&partial,claim,ctx).unwrap().is_none());
 assert!(archive::has_verified_original_scope_v17(&partial,claim,ctx).is_err());
 assert_eq!(family_state.state_root(),original_root);
 let mut wrong=ctx;wrong.blue_score+=1;
 let mut read=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&active).unwrap();
 let before=read.state.state_root();let journal=read.entries.len();
 assert!(read.bind_native_original_input_context_v19(&wrong,&context).is_err());
 assert_eq!(read.state.state_root(),before);assert_eq!(read.entries.len(),journal);
 let absent=prepare_paired_original_v1(&read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
 assert!(absent.input_footprint_v12.is_none());drop(absent);
 read.bind_native_original_input_context_v19(&ctx,&context).unwrap();
 let actual=prepare_paired_original_v1(&read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
 assert!(actual.input_footprint_v12.is_some());drop(actual);drop(read);drop(partial);drop(context);
 let born=super::super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&accepted,claim,ctx).unwrap().unwrap();
 assert!(borsh::object_length(&born.inputs()).unwrap()as u64<=receipt.input_capacity_v1(),"final full fold rechecks actual filtered Source8 generation");
 assert!(archive::has_verified_original_scope_v17(&accepted,claim,ctx).unwrap());
 if let Some(parent)=source_history.as_ref(){
  let parent_receipt=finance::verified_original_archive_reservation_v1(&accepted,parent.claim,parent.point).unwrap().unwrap();
  let parent_h=parent_receipt.material_until_v1();let child_h=receipt.material_until_v1();assert!(child_h>parent_h);
  let mut clocks=active.clone();clocks.tir_funded_originals.clear();clocks.tir_public_originals.clear();
  clocks.fp_claim_answers.clear();clocks.fp_winning_carriers.clear();clocks.carrier_input_spends.clear();
  clocks.prefix_dependencies_v1=Default::default();clocks.history_custody_pledges.clear();
  clocks.public_original_bindings.clear();clocks.producer_custody_delegations.clear();clocks.own_attempt_class=None;
  let step=|state:&PalwChainStateV2,daa:u64,nonce:u64|{
   let tip=state.last_point.unwrap();let point=PalwBlockContextV2{block:h(nonce),daa_score:daa,blue_score:tip.blue_score+1,subsidy:ctx.subsidy};
   let(next,delta)=apply_palw_transition_v2_with_extras(state,&family_params,&point,&[],None,false,false,false,false,&clocks).unwrap();(next,delta,point)
  };
  let mut state=PalwStateCarriageV2::from_state(&accepted).into_state(&family_params,Some(accepted.state_root())).unwrap();
  let budget=state.staged_courts.len()+state.deadlines.len()+state.court_deadlines.len()+state.court_close_deadlines.len()+4*state.claims.len()+8;
  let mut steps=0usize;
  // Follow actual due clocks; jumping to H would postpone Final until H.
  while state.last_point.unwrap().daa_score<parent_h{
   assert!(steps<budget,"native due clocks failed to reach parent horizon");steps+=1;
   let now=state.last_point.unwrap().daa_score;
   let next=state.deadlines.iter().map(|(at,_)|*at).chain(state.staged_due.iter().map(|(at,_)|*at))
    .chain(state.court_deadlines.iter().map(|(at,_)|*at)).chain(state.court_close_deadlines.iter().map(|(at,_)|*at))
    .filter(|at|*at>now&&*at<parent_h).min().unwrap_or(parent_h);
   let(next_state,_,_)=step(&state,next,0x19000000+steps as u64);state=next_state;
  }
  assert_eq!(state.last_point.unwrap().daa_score,parent_h);
  let released=finance::verified_original_archive_reservation_v1(&state,parent.claim,parent.point).unwrap().unwrap();
  assert!(released.released_v1(),"actual H transfers parent ownership to funded child");
  assert!(finance::with_verified_ancestor_descriptor_v19(&state,parent.claim,parent.point.block,parent.point.daa_score,parent.point.blue_score,parent_h,|d|{d.recheck_native_v19(&state)?;Ok(())}).unwrap().is_some(),"H remains inside inclusive material interval");
  assert!(archive::has_verified_original_scope_v17(&state,parent.claim,parent.point).unwrap());
  let at_h=PalwStateCarriageV2::from_state(&state).into_state(&family_params,Some(state.state_root())).unwrap();
  assert!(archive::has_verified_original_scope_v17(&at_h,parent.claim,parent.point).unwrap());
  // Negative corruption changes only an actual financial index, never Claims.
  let lease_roots:Vec<_>=state.kernel_route_state_v1().unwrap().aux.iter().filter_map(|((table,key),raw)|{
   if *table!=crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1||key.len()!=84||key.first()!=Some(&b'I'){return None}
   let root=borsh::from_slice::<crate::palw_claim_local_context_nodes_v2::ComponentRootV2>(raw).ok()?;
   (root.spec.owner==17&&root.spec.table==3&&root.rows>0).then_some(key.clone())
  }).collect();
  assert_eq!(lease_roots.len(),1,"one exact prior-source lease root");
  let mut damaged=TransitionBuilder::try_new(&state,&family_params,false,false,false,false,&clocks).unwrap();
  damaged.claim_local_lock_point_v6=state.last_point;
  damaged.write_kernel_row_encoded_v1::<crate::palw_claim_local_context_nodes_v2::ComponentRootV2>(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1,&lease_roots[0],None).unwrap();
  let damaged_root=damaged.state.state_root();let damaged_journal=damaged.entries.len();
  assert!(archive::has_verified_original_scope_v17(&damaged.state,parent.claim,parent.point).is_err());
  assert!(archive::on_native_archive_ownership_changed_v19(&mut damaged,parent.claim,parent.point).is_err(),"missing paid lease cannot authorize cleanup/release");
  assert_eq!(damaged.state.state_root(),damaged_root);assert_eq!(damaged.entries.len(),damaged_journal);
  assert!(PalwStateCarriageV2::from_state(&damaged.state).into_state(&family_params,Some(damaged_root)).is_err());drop(damaged);
  let after_h=parent_h+1;assert!(after_h<=child_h);
  let mut last=None;
  for n in 0..budget{
   let prior=PalwStateCarriageV2::from_state(&state).into_state(&family_params,Some(state.state_root())).unwrap();
   let(next,delta,point)=step(&state,after_h,0x19100000+n as u64);state=next;last=Some((prior,delta,point));
   if !state.funded_escrows_v1()[&parent_receipt.contract_v1()].liability_open{break}
  }
  assert!(!state.funded_escrows_v1()[&parent_receipt.contract_v1()].liability_open,"real native clocks make parent capital clearable while child lease is live");
  assert!(archive::has_verified_original_scope_v17(&state,parent.claim,parent.point).unwrap());
  let(prior,delta,_)=last.unwrap();
  let undone=super::super::revert_delta_v2(&state,&delta,&family_params).unwrap();assert_eq!(undone.state_root(),prior.state_root());
  let replay=super::super::apply_delta_v2(&undone,&delta,&family_params).unwrap();assert_eq!(replay.state_root(),state.state_root());
  let cold=PalwStateCarriageV2::from_state(&replay).into_state(&family_params,Some(replay.state_root())).unwrap();
  assert!(archive::has_verified_original_scope_v17(&cold,parent.claim,parent.point).unwrap());
  finance::with_verified_ancestor_descriptor_v19(&cold,parent.claim,parent.point.block,parent.point.daa_score,parent.point.blue_score,after_h,|d|{
   assert_eq!(d.old_paid_until_v19(),Some(parent_h));d.recheck_native_v19(&cold)?;Ok(())
  }).unwrap().unwrap();
  // The final actual child release may collect the all21 material archive,
  // while the already-paid native eligibility ledger remains authenticated.
  let mut collection=None;
  for n in 0..budget{
   let prior=PalwStateCarriageV2::from_state(&state).into_state(&family_params,Some(state.state_root())).unwrap();
   let(next,delta,point)=step(&state,child_h+1,0x19200000+n as u64);state=next;collection=Some((prior,delta,point));
   if finance::verified_original_archive_reservation_v1(&state,claim,ctx).unwrap().is_none_or(|r|r.released_v1()){break}
  }
  assert!(finance::verified_original_archive_reservation_v1(&state,claim,ctx).unwrap().is_none_or(|r|r.released_v1()),"actual last-child material closure releases its archive ownership");
  assert!(archive::original_provenance_v16(&state,parent.claim,parent.point).unwrap().is_none(),"last material lease does not keep all21 bodies merely for eligibility");
  assert!(super::super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&state,parent.claim,parent.point).unwrap().is_some(),"paid Source8 eligibility survives separate material collection");
  let(prior,delta,_)=collection.unwrap();
  let undone=super::super::revert_delta_v2(&state,&delta,&family_params).unwrap();assert_eq!(undone.state_root(),prior.state_root());
  let replay=super::super::apply_delta_v2(&undone,&delta,&family_params).unwrap();assert_eq!(replay.state_root(),state.state_root());
  let cold=PalwStateCarriageV2::from_state(&replay).into_state(&family_params,Some(replay.state_root())).unwrap();
  assert!(archive::original_provenance_v16(&cold,parent.claim,parent.point).unwrap().is_none());
  assert!(super::super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&cold,parent.claim,parent.point).unwrap().is_some());
 }
}
