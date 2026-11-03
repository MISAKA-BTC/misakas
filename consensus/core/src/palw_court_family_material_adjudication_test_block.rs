{
// Same admitted Prefix candidate/fences. All evidence below comes from the
// cold-restored literal material owner, never TinyExecution's body methods.
use crate::palw_court_shared_tir_root_v1 as roots;
use crate::palw_court_shared_tir_round_v1 as rounds;
use crate::palw_court_shared_tir_bottom_v1 as bottoms;
use crate::palw_court_numerical_transport_v1::NumericalControlRefV1 as N;
let mut court_state=restored.clone();
assert!(active.tir_public_conviction_active);
assert!(family_params.court_outsider_lane_active_v1(ctx.daa_score+1));
let binding=&fx.execution.binding;
let native=crate::palw_court_close_receipt_v1::NativeCourtRulesV1{
    network_domain:network,at:*court_state.last_point().unwrap(),court:fx.rules.court,step_ladder:fx.rules.maximum_leaves,
    prompt_ids_merkle:matches!(fx.rules.prompt_form,crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1),held_regime:false,
    decode_close_convicts_only:false,fp_decode_from_daa:None,cell_closure_from_daa:None,memory_routing_from_daa:None,
    tir_active:true,gen_active:false,task_heads_active:false,improvement_active:false,constraint_active:false,
};
let court=crate::palw_tir_court_v1::PalwTirCourtRulesV1{max_step_leaf_count:native.step_ladder,prompt_form:fx.rules.prompt_form,
    limits:crate::palw_court_v2::palw_tir_court_limits_v1(&native.court)};
let material=crate::palw_court_rooted_numerical_material_v1::RootedNumericalMaterialV1::retain_v1(&court_state,claim).unwrap();
let scope=crate::palw_court_shared_tir_state_v1::context(&court_state,claim).unwrap().scope_id;
let store=material.store_v1(&court_state,claim,scope).unwrap();
let mut root_errors=Vec::new();
let (leaf,mut root)=(0..binding.step_leaf_count).find_map(|leaf|
    match crate::palw_tir_court_v1::build_tir_root_claim_v1(binding,leaf,&store,&court) {
        Ok(root)=>Some((leaf,root)),
        Err(error)=>{root_errors.push((leaf,error.to_string()));None},
    })
    .unwrap_or_else(||panic!("restored native operands support a real numerical root; actual builder refusals: {root_errors:?}"));
crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut root.finalize.binding);
let mut control=roots::SignedSharedTirRootV1{version:1,claim,scope_id:scope,policy_id:roots::shared_tir_root_policy_id_v1(&native).unwrap(),
    leaf,arity:native.court.dissection_arity(),signer:producer,root:Box::new(root),signature:vec![]};
control.signature=mldsa::sign(&kp.signing_key,control.signing_message(network).unwrap().as_byte_slice(),roots::SHARED_TIR_ROOT_CONTEXT_V1,[0x71;32]).unwrap().as_ref().to_vec();
drop(store);drop(material);
let initial_claim=court_state.claim(&claim).unwrap().clone();
let initial_slashed=court_state.bond(&producer).unwrap().slashed;
let mut apply=|state:&PalwChainStateV2,op:crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1|{
    elapsed+=1;
    let last=*state.last_point().unwrap();
    let point=PalwBlockContextV2{daa_score:last.daa_score.checked_add(1).unwrap(),blue_score:last.blue_score.checked_add(1).unwrap(),block:h(0x430000+elapsed),subsidy:0};
    assert!(point.daa_score<crate::palw_court_shared_tir_state_v1::context(state,claim).unwrap().hard_daa,"actual court must fit the unchanged original hard");
    let mut extras=active.clone();let mut rules=native;rules.at=point;extras.native_court_rules_v1=Some(rules);
    let object=PalwConsensusObjectV2::SharedCourtOpV1{op:Box::new(op)};
    let (next,delta)=apply_palw_transition_v2_with_extras(state,&family_params,&point,&[object],None,false,false,false,false,&extras).unwrap();
    let mut undone=next.clone();for entry in delta.entries.iter().rev(){apply_delta_entry(&mut undone,entry,true).unwrap();}
    undone.set_last_point_for_tests(state.last_point().cloned());assert_eq!(undone.state_root(),state.state_root(),"actual signed court delta reverses all owners");
    let bytes=borsh::to_vec(&PalwStateCarriageV2::from_state(&next)).unwrap();
    borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&family_params,Some(next.state_root())).unwrap()
};
let (manifest,parts)=numerical_parts(N::Root(&control),scope,&native);
court_state=apply(&court_state,crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::Numerical{manifest,parts});
let mut path=Vec::new();let mut visited=0u32;
loop{
    visited+=1;assert!(visited<=u32::from(rounds::SHARED_TIR_MAX_PATH_V1 as u16)+2,"finite native dissection depth");
    let prepared=rounds::prepare_shared_tir_round_v1(&court_state,claim,leaf,&path).unwrap();
    let material=crate::palw_court_rooted_numerical_material_v1::RootedNumericalMaterialV1::retain_v1(&court_state,claim).unwrap();
    let store=material.store_v1(&court_state,claim,scope).unwrap();
    if prepared.phase.terminal_range().is_some(){
        let mut evidence=crate::palw_tir_court_v1::build_tir_dissect_bottom_v1(binding,&prepared.phase,&store,&court).unwrap();
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut evidence.binding);
        let mut bottom=bottoms::SignedSharedTirBottomV1{version:1,claim,leaf,root_id:prepared.root_id,path:path.clone(),parent_id:prepared.parent_id,
            filer:outsider(),bottom:Box::new(evidence),signature:vec![]};
        bottom.signature=mldsa::sign(&family_filer_key.signing_key,bottom.signing_message(network).unwrap().as_byte_slice(),bottoms::SHARED_TIR_BOTTOM_CONTEXT_V1,[0x73;32]).unwrap().as_ref().to_vec();
        let (manifest,parts)=numerical_parts(N::Bottom(&bottom),scope,&native);drop(store);drop(material);
        court_state=apply(&court_state,crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::Numerical{manifest,parts});
        let Some(Row::CourtSharedBottom(row))=court_state.staged_court_row_v1(&Key::CourtSharedBottom(bottom.id().unwrap()))else{panic!("signed native court bottom must persist")};
        let mut rules=native;rules.at=*court_state.last_point().unwrap();
        let checked=bottoms::check_shared_tir_bottom_v1(&court_state,&family_params,&rules,&row.control).unwrap();
        assert_eq!(checked.outcome(),&bottoms::SharedTirBottomOutcomeV1::NoMismatchFound);
        break;
    }
    let msg=crate::palw_tir_court_v1::build_tir_dissect_round_v1(binding,&prepared.phase,fx.layout.h_tile,&store,&court).unwrap();
    let mut round=rounds::SignedSharedTirRoundV1{version:1,claim,leaf,root_id:prepared.root_id,path:path.clone(),parent_id:prepared.parent_id,
        signer:producer,round:Box::new(msg),signature:vec![]};
    round.signature=mldsa::sign(&kp.signing_key,round.signing_message(network).unwrap().as_byte_slice(),rounds::SHARED_TIR_ROUND_CONTEXT_V1,[0x72;32]).unwrap().as_ref().to_vec();
    let (manifest,parts)=numerical_parts(N::Round(&round),scope,&native);drop(store);drop(material);
    court_state=apply(&court_state,crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::Numerical{manifest,parts});path.push(0);
}
assert_eq!(court_state.claim(&claim),Some(&initial_claim),"honest local court result does not replace Gate C authentication");
assert_eq!(court_state.bond(&producer).unwrap().slashed,initial_slashed);
let semantic=crate::palw_court_rooted_semantic_v1::palw_retain_rooted_family_table_v1(&court_state,claim).unwrap().unwrap();
assert_eq!(semantic.table_v1(),table);
assert_eq!(&**crate::palw_court_rooted_semantic_v1::palw_retain_rooted_tokenizer_raw_v1(&court_state,claim).unwrap().as_ref().unwrap(),raw);
}
