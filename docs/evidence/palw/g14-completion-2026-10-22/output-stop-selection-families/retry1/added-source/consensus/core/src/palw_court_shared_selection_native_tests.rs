// Actual original-rule interpreter fixtures; existing verified-half source
// pledge setup is not mined/funded admission or a model correctness result.
#[test]
fn shared_selection_family_actual_rules_and_runtime_tiles_recheck(){
    use crate::palw_court_shared_tir_builder_store_v1::{SharedTirBuilderStoreV1 as Store,selection_terminal::{
        NativeSelectionUniverseV1 as Universe,SelectionTargetV1 as Target,SignedSelectionIntentV1 as Intent,
        SelectionDecisionV1 as Decision,SELECTION_INTENT_CONTEXT_V1 as CONTEXT}};
    use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as Graph,SourceGraphBudgetV3 as Budget};
    use crate::palw_court_shared_tir_state_v1 as shared;
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
    use crate::palw_tir_court_v1::{PalwTirSelectionRuleV1 as Rule,tir_logits_event_disclosure_v1,PalwTirTraceEventDisclosureV1};
    for kind in 0..4 {for wrong in [false,true] {
        if kind==3&&wrong{continue;}
        let pos=if kind==2{1}else{0};
        let fx=if kind==3 {crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1()}
            else{crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(1,|job|{
                if kind==1 {job.temperature_q=1<<24;job.sampling_seed=[7;32];}
                if kind==2 {job.version=crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;
                    job.decode=Some(crate::palw_decode_pipeline_v4::DecodeConfigV4{repeat_penalty_q:4*65536,penalty_window:4,
                        frequency_penalty_q:2<<24,presence_penalty_q:2<<24,..crate::palw_decode_pipeline_v4::DecodeConfigV4::NOOP});}
            },wrong.then_some(pos))};
        let frame=&fx.frame;let p=frame.plan_v1();let claim=p.original_claim;
        let (mut state,params,extras)=world(&fx,frame);
        let key=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([121;32]);state.bonds.get_mut(&outsider()).unwrap().pubkey=key.verification_key.as_ref().to_vec();
        let state=bind_clock_service(&state,frame,&params,&extras);
        let source=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),
            frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
        let profile=shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();let scope=set.scope_id().unwrap();
        let c=shared::SharedTirContextV1{version:1,claim,allocation_id:p.allocation_id,source_id:p.source_id,rules_id:p.rules_id,profile_id:profile,
            scope_id:scope,queries:set.len(),original_accepted_daa:p.terms.accepted_daa,hard_daa:p.terms.accepted_daa+10000,material_until_daa:p.terms.material_until_daa};
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        b.write_staged_row_v1(Key::CourtSharedContext(claim),Some(Row::CourtSharedContext(Box::new(c)))).unwrap();
        // Test local graph capacity derives from actual grouped transport; it
        // does not select a production fee/profile/cap or guarantee supply.
        let upper=set.signed_data_transport_upper().unwrap();let roles=upper.maximum_custodians;
        let edges=upper.fresh_responses.checked_mul(roles+1).unwrap().checked_add(upper.streams.checked_mul(2).unwrap()).unwrap().checked_add(upper.parts).unwrap();
        let budget=Budget{max_queries:set.len(),max_roles:roles,max_vertices:upper.signed_controls,max_edges:edges};
        let graph=Graph::derive_native_v3(frame,&source,profile,budget).unwrap();
        let universe=Universe::derive_native_v1(frame,&source,&graph,profile).unwrap();
        assert_eq!(universe.len_v1(),u64::from(source.decode_count_v1())*u64::from(source.vocab_v1()));
        assert_eq!(universe.target_at_v1(0),Some(Target{position:0,beat:0}));
        assert_eq!(universe.target_at_v1(universe.len_v1()-1),Some(Target{position:source.decode_count_v1()-1,beat:source.vocab_v1()-1}));
        assert!(universe.target_at_v1(universe.len_v1()).is_none());
        let rule=Rule::of_job_v1(&frame.original_v1().job).unwrap();let committed=fx.execution.generated[pos as usize];
        let beat=if kind==3{if committed<4096{4096}else{0}}else if wrong{
            rule.select_v1(&fx.execution.generated[..pos as usize],&fx.execution.rows[pos as usize],None).unwrap()
        }else{committed};
        if wrong{assert_ne!(beat,committed);}
        let visits=graph.nodes_v3().count() as u64;
        let duty=universe.project_native_v1(Target{position:pos,beat},visits).unwrap();
        let tiles=u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        assert_eq!(duty.dependencies_v1().len() as u64,(tiles+1)*roles);
        for dep in duty.dependencies_v1(){
            let expected=graph.nodes_v3().find(|n|match n.kind_v3(){
                crate::palw_court_source_material_graph_v3::SourceMaterialNodeKindV3::Finish{target:crate::palw_court_source_material_graph_v3::SourceMaterialTargetV3::FullIds,role}=>dep.tile.is_none()&&*role==dep.role,
                crate::palw_court_source_material_graph_v3::SourceMaterialNodeKindV3::Finish{target:crate::palw_court_source_material_graph_v3::SourceMaterialTargetV3::Unit(n),role}=>
                    *role==dep.role&&dep.tile.is_some_and(|t|set.request_at(*n).unwrap()==(Q::TraceTile{row:pos,tile:t as u32})),_=>false,
            }).unwrap();
            assert_eq!(dep.logical,expected.id_v3());assert_eq!(dep.absolute,graph.absolute_packet_id_v3(expected.id_v3()));
        }
        // Exact local projection owner accounting only. Universe/Source/graph
        // and builder scratch have independent tickets and are not this quota.
        use crate::palw_court_shared_tir_builder_store_v1::selection_terminal::{NativeSelectionDutyV1,SelectionDependencyV1};
        let required=(std::mem::size_of::<NativeSelectionDutyV1>() as u64).checked_add(
            (duty.dependencies_v1().len() as u64).checked_mul(std::mem::size_of::<SelectionDependencyV1>() as u64).unwrap()).unwrap();
        for limit in [required-1,required]{
            let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:limit,database_bytes:0});
            let projected=universe.project_native_reserved_v1(Target{position:pos,beat},visits,|q|{assert_eq!(q,required);checker.reserve_requests_v1(q,0).map(Some)});
            if limit==required {let held=projected.unwrap();assert_eq!(checker.outstanding(),(required,0));assert_eq!(held.absolute_id_v1(),duty.absolute_id_v1());drop(held);}
            else{assert!(matches!(projected,Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));}
            assert_eq!(checker.outstanding(),(0,0));
        }
        assert!(matches!(universe.project_native_v1(Target{position:pos,beat},0),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        assert!(universe.project_native_v1(Target{position:source.decode_count_v1(),beat},visits).is_err());
        assert!(universe.project_native_v1(Target{position:pos,beat:source.vocab_v1()},visits).is_err());
        let rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context,&fx.execution.rows).unwrap();
        let ids=source.retain_complete_ids_v1(rows,&fx.execution.generated).unwrap();
        let mut units=Vec::new();
        for tile in 0..tiles {
            let PalwTirTraceEventDisclosureV1::Tiled{row_root,row_opening,tile_lanes,tile_opening,..}=
                tir_logits_event_disclosure_v1(&fx.execution.binding,&fx.execution.rows,&fx.execution.generated,pos,u8::try_from(tile).expect("actual fixture tile fits existing wire")).unwrap() else{panic!("actual tiled fixture")};
            units.push(source.authenticate_v1(&Q::TraceTile{row:pos,tile:tile as u32},
                &R::TraceTile{row_root,row_opening,lanes:tile_lanes,opening:tile_opening},Some(&ids)).unwrap());
        }
        let store=Store::new(&b.state,claim,scope,&units,&[],Some(&ids)).unwrap();
        let rules=crate::palw_court_close_receipt_v1::NativeCourtRulesV1{network_domain:extras.tir_public_network_domain_v1.unwrap(),at:at(frame,1),
            court:fx.rules.court,step_ladder:fx.rules.maximum_leaves,prompt_ids_merkle:false,held_regime:false,decode_close_convicts_only:false,
            fp_decode_from_daa:None,cell_closure_from_daa:None,memory_routing_from_daa:None,tir_active:true,gen_active:false,task_heads_active:false,improvement_active:false,constraint_active:false};
        let mut intent=Intent{version:1,claim,scope_id:scope,policy_id:crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&rules).unwrap(),
            duty_id:duty.absolute_id_v1(),position:pos,beat,material_id:store.selection_material_id_v1(pos,beat).unwrap(),filer:outsider(),signature:vec![]};
        let sign=|i:&mut Intent|{i.signature=libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key,i.signing_message(rules.network_domain).unwrap().as_byte_slice(),CONTEXT,[122;32]).unwrap().as_ref().to_vec();};
        sign(&mut intent);let before=b.state.state_root();
        let evidence=store.build_selection_terminal_v1(&b.state,&params,&rules,&intent,&duty).unwrap();
        let decision=evidence.recheck(&b.state,&params,&rules).unwrap();
        assert_eq!(matches!(decision,Decision::Fault{..}),wrong);
        // Direct legacy native primitive over real execution material: exact
        // same proof id/outcome as this new material/dependency adapter.
        let pin=crate::palw_step_refute::tiled_decode_pin_v1(&fx.execution.binding.job_context,&fx.execution.rows,&fx.execution.generated,pos,beat).unwrap();
        let mut proof=crate::palw_court_v2::PalwCourtVerdictProofV2::TirDecodeTokenTiled{binding:Box::new(fx.execution.binding.clone()),pin};proof.tir_strip_program_v1();
        let direct=crate::palw_tir_objective_v1::adjudicate_original_decode_v1(&b.state,claim,&proof,&rules.court,rules.step_ladder,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,frame.plan_v1().terminal.maximum_witness_bytes,&rule).unwrap();
        match (decision,direct){
            (Decision::Fault{proof,position,..},crate::palw_tir_objective_v1::PalwTirObjectiveOutcomeV1::Convicts(f))=>{
                assert_eq!(proof,f.proof_id_v1());assert_eq!(position,u64::from(pos));}
            (Decision::PredicateHolds{proof,..},crate::palw_tir_objective_v1::PalwTirObjectiveOutcomeV1::PredicateHolds{proof_id,..})=>assert_eq!(proof,proof_id),
            _=>panic!("native primitive and borrowed adapter differ"),
        }
        let no_ids=Store::new(&b.state,claim,scope,&[],&[],None).unwrap();
        assert!(no_ids.build_selection_terminal_v1(&b.state,&params,&rules,&intent,&duty).is_err());
        for missing_tile in [committed/4096,beat/4096]{
            let sparse=units.iter().filter(|u|u.request_v1()!=(Q::TraceTile{row:pos,tile:missing_tile})).cloned().collect::<Vec<_>>();
            let missing=Store::new(&b.state,claim,scope,&sparse,&[],Some(&ids)).unwrap();
            assert!(missing.build_selection_terminal_v1(&b.state,&params,&rules,&intent,&duty).is_err());
        }
        let mut invalid=intent.clone();invalid.beat=source.vocab_v1();sign(&mut invalid);
        assert!(store.build_selection_terminal_v1(&b.state,&params,&rules,&invalid,&duty).is_err());
        let mut invalid=intent.clone();invalid.scope_id=h(0xFB01);sign(&mut invalid);
        assert!(store.build_selection_terminal_v1(&b.state,&params,&rules,&invalid,&duty).is_err());
        let mut bad_signature=intent.clone();bad_signature.signature[0]^=1;
        assert!(store.build_selection_terminal_v1(&b.state,&params,&rules,&bad_signature,&duty).is_err());
        let mut late=rules.clone();late.at.daa_score=p.terms.accepted_daa+10000;
        assert!(store.build_selection_terminal_v1(&b.state,&params,&late,&intent,&duty).is_err());
        let mut replaced=b.state.clone();replaced.bonds.get_mut(&outsider()).unwrap().pubkey=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([123;32]).verification_key.as_ref().to_vec();
        assert!(evidence.recheck(&replaced,&params,&rules).is_err());assert_eq!(b.state.state_root(),before);
    }}
}
