{
    use archive_service_view_v11::{NativeFinalBranchScopeV11 as Scope,MissingFinalArchiveActorScopeV11 as Gap,FutureArchiveParticipantPolicyV11 as Future};
    let mut seen=std::collections::BTreeSet::new();
    let (mut signer,mut source,mut retention,mut transfer,mut holding)=(0usize,0usize,0usize,0usize,0usize);
    let traversal=view.visit_branch_obligations_v11(|branch|{
        let packet=branch.packet_v11();
        assert!(seen.insert(packet.id),"exact native branch map is one entry per final-bound packet");
        assert_eq!(packet,view.packet_v11(&packet.id)?);
        // Actual assignment is checked as a separate fact, never assumed equal
        // to earliest release or to exclusive service_before.
        assert!((packet.release_daa..packet.service_before).contains(&branch.actual_assigned_daa_v11()));
        match branch.scope_v11(){
            Scope::ExistingSourceResponse{signer:actor,turn,custody}=>{
                signer+=1;
                assert!(custody.liable_signers_v1().contains(actor));
                assert_eq!(custody.pledge_v1().id_v1(),view.original_custody_v11().pledge_v1().id_v1());
                assert_eq!(custody.terms_v1(),view.original_custody_v11().terms_v1());
                assert!(*turn<crate::palw_funded_original_coordinator_v1::palw_funded_original_schedule_v1(&active).unwrap().admission_v1().unwrap().service_profile().packets_per_signer);
            },
            Scope::ProtocolSourceQueryCapacity{logical}=>{
                source+=1;
                let graph=req.source_graph_v1().unwrap();
                assert!(graph.nodes_v3().any(|n|n.id_v3()==*logical));
                assert_eq!(packet.id,graph.absolute_packet_id_v3(*logical));
            },
            Scope::ProtocolRetentionCapacity{segment}=>{
                retention+=1;assert_eq!(packet.cost.io,0);
                assert!(segment.is_some(),"real TFO2 uses segmented capacity; no anonymous operator");
            },
            Scope::FamilyTargetTransfer{consent,target}=>{
                transfer+=1;
                assert_eq!(packet.cost.io,crate::palw_g14_values_v1::PALW_G14_SOURCE_PACKET_COST_V1.io);
                assert!(target.packets.contains(&packet.id));
                assert_eq!(consent.terms.claim,claim);assert_eq!(consent.terms.source,facts.scope_v11().1);
                assert_eq!(consent.terms.relative_graph,facts.scope_v11().5);
                assert_eq!(consent.terms.respond_until_daa,hard);
                assert!(consent.terms.material_until_daa>=hard);
                assert!(consents.iter().any(|actual|actual==*consent),"borrows exact original signatures and signed horizons");
            },
            Scope::FamilyContractHoldingCapacity{consent}=>{
                holding+=1;assert_eq!(packet.cost.io,0);
                assert!(consents.iter().any(|actual|actual==*consent));
            },
        }
        Ok(())
    })?;
    assert_eq!(traversal.emitted_packet_records,seen.len() as u64);
    assert_eq!(traversal.ordered_index_queries,3*seen.len() as u64);
    assert_eq!(traversal.family_target_packet_ids,transfer as u64);
    assert_eq!(traversal.family_target_packet_ids,view.actual_family_rows_v11()
        .flat_map(|r|&r.targets).map(|t|t.packets.len() as u64).sum::<u64>());
    assert_eq!(traversal.family_packet_classifications,(transfer+holding) as u64);
    assert_eq!(traversal.original_branch_records,(signer+source+retention) as u64);
    assert_eq!(&seen,view.packet_ids_v11());
    assert_eq!(signer+source+retention+transfer+holding,facts.packets_v11().len());
    assert!(signer>0&&source>0&&retention>0&&transfer>0&&holding>0);
    assert_eq!(view.future_participant_policy_v11(),Future::StandingBondAndSignedOpeningCheckedAtActualParticipation);
    for gap in [Gap::Source16ArchiveStorageFunding,Gap::ProducerFpLiabilityCoverage,Gap::NativeMetadataWorkTariff,Gap::FutureSourceLifecycleCoverage]{
        assert!(facts.missing_v11().contains(&gap),"old source/family signatures cannot close extra archive/FP coverage or unknown work");
    }
    assert_eq!(facts.binding_body_v11().canonical_bytes,borsh::object_length(view.binding_v11()).unwrap() as u64);
    let stored=facts.source16_body_facts_v11();
    assert_eq!(stored,[(6,facts.binding_body_v11()),(7,facts.final_packet_vec_body_v11())]);
    let refs=facts.packets_v11().iter().collect::<Vec<_>>();
    assert_eq!(borsh::to_vec(&refs).unwrap(),borsh::to_vec(facts.packets_v11()).unwrap(),"Source16 tag7 reference-vector bytes equal owned ordered packet bytes");
}
