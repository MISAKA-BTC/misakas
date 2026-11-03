{
// Included only after the full native TFO2 original transition has committed.
// Materials cross the real object-250 dispatcher, signature checks and journal.
if let (Some(raw),Some(table))=(&family_raw,&family_table) {
    use crate::palw_court_family_material_v1 as fm;
    use crate::palw_court_shared_material_intake_v1::MaterialTargetV1 as MT;
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    include!("palw_court_family_independent_draft_native_test_block.rs");
    let mut material_state=accepted.clone();
    let source=fx.frame.source_identity_native_v2(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
    let profile=crate::palw_court_shared_tir_state_v1::context(&accepted,claim).unwrap().profile_id;
    let requests=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(&fx.frame,&source,profile).unwrap();
    let row_hashes:Vec<_>=fx.execution.rows.iter().enumerate().map(|(row,lanes)|crate::palw_step_refute::tiled_logits_row_root_v1(&fx.execution.binding.job_context.context_hash(),row as u32,lanes).unwrap()).collect();
    let rows_root=crate::palw_step_leg::step_merkle_root_v1(&row_hashes).unwrap();
    let response=|query:Q|->R {match query {
        Q::StepNode{level,index}=>{let(frontier,siblings)=fx.execution.step_node(level,index).unwrap();R::StepNode{frontier,siblings}}
        Q::StepLeaf{index}=>R::StepLeaf{preimage:fx.execution.step_leaf(index).unwrap(),opening:fx.execution.step_opening(index).unwrap()},
        Q::StepRun{first,count}=>R::StepRun{preimages:(first..first+u64::from(count)).map(|i|fx.execution.step_leaf(i).unwrap()).collect(),range:crate::palw_step_leg::PalwStepRangeOpeningV1{first_leaf_index:first,leaf_hashes:fx.execution.hashes[first as usize..first as usize+count as usize].to_vec(),siblings:fx.execution.step_range_siblings(first,u64::from(count)).unwrap()}},
        Q::RowsNode{level,index}=>{let(frontier,siblings)=fx.execution.row_node(level,index).unwrap();R::RowsNode{frontier,siblings}}
        Q::TileNode{row,level,index}=>{
            let tile_hashes:Vec<_>=fx.execution.rows[row as usize].chunks(crate::palw_step_refute::PALW_LOGITS_TILE_LANES).enumerate().map(|(tile,lanes)|crate::palw_step_refute::tiled_logits_tile_leaf_v1(&fx.execution.binding.job_context.context_hash(),row,tile as u32,lanes)).collect();
            let(frontier,siblings)=crate::palw_tir_court_v1::palw_tir_step_node_parts_v1(&tile_hashes,level,index).unwrap();
            R::TileNode{row_root:row_hashes[row as usize],row_opening:crate::palw_step_leg::step_opening_v1(&row_hashes,u64::from(row)).unwrap(),frontier,siblings}
        }
        Q::TraceTile{row,tile}=>{let pin=fx.execution.row_pin(row,tile*crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u32).unwrap();R::TraceTile{row_root:pin.row_root,row_opening:pin.row_opening,lanes:pin.beat_tile_lanes,opening:pin.beat_opening}}
        Q::Ids{offset}=>R::Ids{rows_root,ids:fx.execution.generated[offset as usize..fx.execution.generated.len().min(offset as usize+crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize)].to_vec()},
    }};
    // Fold real empty accepting blocks up to each exact native assignment;
    // no holding row or clock jump substitutes for a material transfer.
    let mut elapsed=0u64;
    let mut completed_roles=std::collections::BTreeSet::new();
    let mut checked_open_scope=false;
    for (consent,inventory) in consents.iter().zip(inventories.iter()) {
        if consent.terms.role==FamilySupplierRoleV1::SemanticTable {
            let supplied=fm::read_authenticated_family_table_inventory_v1(&material_state,claim).unwrap().expect("actual original supplied its table");
            assert_eq!(supplied.bytes,crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(table));
            assert_eq!(supplied.carrier,actual.outpoint(),"table provenance is the actual full TFO2 carrier");
            let row=fs::family_contract_for_material_v1(&material_state,consent.id_v1().unwrap()).unwrap().unwrap();
            assert_eq!(row.completed.len(),inventory.targets_v1().len());
            assert!(matches!(row.fee_state,FamilySupplierFeeStateV1::Held),"birth supply never accelerates the signed fee horizon");
            assert!(completed_roles.insert(consent.terms.role));
            continue;
        }
        let mut targets=inventory.targets_v1().to_vec();
        // Complete IDs must authenticate before any row/tile opening is accepted.
        targets.sort_by_key(|(target,_)|!matches!(target,fm::FamilyMaterialTargetV1::Input(MT::FullIds)));
        for (target,_) in targets {
            let body=match target {
                fm::FamilyMaterialTargetV1::SemanticTable=>crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(table),
                fm::FamilyMaterialTargetV1::TokenizerRaw=>borsh::to_vec(raw).unwrap(),
                fm::FamilyMaterialTargetV1::Input(MT::Parameter{leaf})=>borsh::to_vec(&fx.execution.param_opening(leaf).unwrap()).unwrap(),
                fm::FamilyMaterialTargetV1::Input(MT::PromptFlat)=>borsh::to_vec(&fx.execution.prompt).unwrap(),
                fm::FamilyMaterialTargetV1::Input(MT::Unit{ordinal})=>borsh::to_vec(&response(requests.request_at(ordinal).unwrap())).unwrap(),
                fm::FamilyMaterialTargetV1::Input(MT::FullIds)=>{
                    let mut bytes=Vec::new();
                    for offset in (0..fx.execution.generated.len() as u32).step_by(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {bytes.extend_from_slice(&borsh::to_vec(&response(Q::Ids{offset})).unwrap());}
                    bytes
                }
                _=>continue,
            };
        let contract=consent.id_v1().unwrap();
        let digest=crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
        let draft=fm::family_material_draft_id_v1(contract,target,producer,body.len() as u64,digest).unwrap();
        let mut actions=vec![fm::FamilyMaterialActionV1::Begin{total_bytes:body.len() as u64,body_hash:digest}];
        for (part,bytes) in body.chunks(fm::FAMILY_MATERIAL_PART_BYTES_V1).enumerate() {
            actions.push(fm::FamilyMaterialActionV1::Part{draft,offset:(part*fm::FAMILY_MATERIAL_PART_BYTES_V1) as u64,bytes:bytes.to_vec()});
        }
        actions.push(fm::FamilyMaterialActionV1::Finish{draft});
        let catalog=fs::family_contract_for_material_v1(&material_state,contract).unwrap().unwrap();
        let target_row=catalog.targets.iter().find(|t|t.id==inventory.target_id_v1(target).unwrap()).unwrap();
        let transfer_ids=target_row.packets.clone();drop(catalog);
        for action in actions {
            let ordinal=match &action {fm::FamilyMaterialActionV1::Begin{..}=>0,fm::FamilyMaterialActionV1::Part{offset,..}=>usize::try_from(offset/(fm::FAMILY_MATERIAL_PART_BYTES_V1 as u64)+1).unwrap(),fm::FamilyMaterialActionV1::Finish{..}=>transfer_ids.len()-1,_=>unreachable!()};
            let assigned=material_state.court_service_state_v1().unwrap().book.assigned_daa(&transfer_ids[ordinal]).unwrap();
            let mut daa=material_state.last_point().unwrap().daa_score;
            assert!(assigned>daa,"rooted material assignment must remain in the future");
            while daa+1<assigned {
                daa+=1;elapsed+=1;
                let beat=PalwBlockContextV2{daa_score:daa,blue_score:ctx.blue_score+elapsed,block:h(0x430000+elapsed),subsidy:0};
                material_state=apply_palw_transition_v2_with_extras(&material_state,&family_params,&beat,&[],None,false,false,false,false,&active).unwrap().0;
            }
            elapsed+=1;
            let signed=fm::sign_family_material_v1(fm::SignedFamilyMaterialV1{version:1,contract,target,submitter:producer,action,signature:vec![]},network,
                |_,message,context|Some(mldsa::sign(&kp.signing_key,message,context,seed).unwrap().as_ref().to_vec())).unwrap();
            let next_ctx=PalwBlockContextV2{daa_score:assigned,blue_score:ctx.blue_score+elapsed,block:h(0x420000+elapsed),subsidy:0};
            let object=PalwConsensusObjectV2::CourtFamilyMaterialV1{control:Box::new(signed)};
            let (physical,_,reserved,_)=crate::palw_court_transport_v1::family_material_accounted_v2(&material_state,&family_params,assigned,&object).unwrap().unwrap();
            assert!(reserved,"every actual primary transfer consumes its exact original-funded service opportunity");
            assert!(physical.work>0);
            let before=material_state.clone();
            let historical_before=if scoped {Some(super::super::palw_claim_local_rewards_v1::frames(&before).count())}else{None};
            let (next,delta)=apply_palw_transition_v2_with_extras(&material_state,&family_params,&next_ctx,std::slice::from_ref(&object),None,false,false,false,false,&active).unwrap();
            if let Some(count)=historical_before {
                assert_eq!(super::super::palw_claim_local_rewards_v1::frames(&next).count(),count,"pure authenticated family control must not retain another full-world F frame; delta={:?}",delta.entries.iter().map(std::mem::discriminant).collect::<Vec<_>>());
                let checkpoint=palw_claim_local_checkpoint_snapshot_v2(&next).unwrap().expect("same-point resolved checkpoint");
                assert_eq!(checkpoint.point,next_ctx);
                let id=crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1::ChainClaim{claim};
                let record=checkpoint.records.get(&id).expect("actual original remains in checkpoint");
                if record.liability_closed {
                    assert!(record.voided&&record.material.is_empty(),"the closed void drops its liability materials");
                }else{
                    assert!(!record.material.is_empty(),"the open original retains its scoped material");
                    checked_open_scope=true;
                    use super::super::palw_claim_local_material_projection_v2 as projection;
                    let native=projection::coordinates(&next).unwrap();
                    let aux=&next.kernel_route.as_ref().unwrap().aux;
                    for row in native.iter().filter(|row|row.claim==claim) {
                        let (material,_)=projection::coordinate_identity_v3(row);
                        let bytes=&aux[&(row.table,row.key.to_vec())];
                        assert!(record.material.contains(&material),"current family row must be in actual owner manifest: table={} key={:?}",row.table,row.key);
                        assert_eq!(checkpoint.material.get(&material).map(Vec::as_slice),Some(bytes.as_slice()));
                    }
                    // Confirm each distinct native table identity independently of
                    // the coordinate projection's owner lookup.
                    let child_key=fm::material_aux_key_v1(4,claim,None);
                    let child=&aux[&(fm::FAMILY_MATERIAL_TABLE_V1,child_key)];
                    let child_material=crate::palw_claim_local_v1::palw_liability_material_id_v1(child);
                    assert!(record.material.contains(&child_material));
                    if let Some(history)=source_history.as_ref() {
                        let parent_key=fm::material_aux_key_v1(4,history.claim,None);
                        assert!(native.iter().any(|row|row.table==fm::FAMILY_MATERIAL_TABLE_V1&&row.key==parent_key.as_slice()&&row.claim==history.claim));
                        let parent=&aux[&(fm::FAMILY_MATERIAL_TABLE_V1,parent_key)];
                        let parent_material=crate::palw_claim_local_v1::palw_liability_material_id_v1(parent);
                        assert!(!record.material.contains(&parent_material),"the retained parent table belongs to its own claim");
                    }
                }
            }
            assert!(delta.entries.iter().all(|entry|!matches!(entry,
                PalwDeltaEntryV2::KernelRouteRow{table,key,..} if *table==FAMILY_SUPPLIER_TABLE_V1&&key.len()==64)),
                "an authenticated target completion never journals the immutable signed consent/catalogue");
            assert_eq!(before.kernel_route().unwrap().aux.get(&(FAMILY_SUPPLIER_TABLE_V1,contract.as_byte_slice().to_vec())),
                       next.kernel_route().unwrap().aux.get(&(FAMILY_SUPPLIER_TABLE_V1,contract.as_byte_slice().to_vec())));
            let mut undone=next.clone();
            for entry in delta.entries.iter().rev(){apply_delta_entry(&mut undone,entry,true).unwrap();}
            undone.set_last_point_for_tests(before.last_point().cloned());
            assert_eq!(undone.state_root(),before.state_root(),"production material journal restores exact prior collateral/material/completion");
            material_state=next;
        }
        let row=fs::family_contract_for_material_v1(&material_state,contract).unwrap().unwrap();
        assert_eq!(row.completed.get(&inventory.target_id_v1(target).unwrap()),Some(&digest));
        assert!(matches!(row.fee_state,FamilySupplierFeeStateV1::Held));
        let retained=fm::read_authenticated_family_material_classified_v1(&material_state,inventory,target).unwrap().unwrap();
        assert_eq!(retained.bytes,body);
        }
        let row=fs::family_contract_for_material_v1(&material_state,consent.id_v1().unwrap()).unwrap().unwrap();
        assert_eq!(row.completed.len(),inventory.targets_v1().len(),"every actual required target of this role completed through authenticated ingress");
        assert!(completed_roles.insert(consent.terms.role));
    }
    if scoped {assert!(checked_open_scope,"the open original's scoped projection was checked before BindTimeout");}
    assert_eq!(completed_roles.len(),consents.len(),"all four roles supplied through actual original or multipart ingress");
    let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&material_state)).unwrap();
    let restored=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&family_params,Some(material_state.state_root())).unwrap();
    assert_eq!(restored.state_root(),material_state.state_root(),"cold restore re-authenticates stored material and exact completed targets");
    let restored=include!("palw_court_family_material_legacy_numerical_ingress_test_block.rs");
    include!("palw_court_family_material_adjudication_test_block.rs");
    include!("palw_court_family_material_semantic_adjudication_test_block.rs");
}

}
