//! Real pruning-proof/trusted-anticone/UTXO/PALW import for the experimental
//! stage2 candidate. No genesis body replay or pruning-depth replacement.
use super::*;
use kaspa_consensus_core::{pruning::PruningProofMetadata,trusted::{TrustedHeader,TrustedGhostdagData}};
use kaspa_p2p_lib::convert::model::trusted::{TrustedDataEntry,TrustedDataPackage};
pub(super) async fn exercise(net:&Net,pending:Option<Hash64>){
    use crate::pipeline::virtual_processor::tests::g14_kernel_route_e2e::material_typed_ingress_v4::source_manifest::pruned_import::{headers_after_point,import_snapshot};
    let source=&net.chain.ctx.consensus;let point=source.pruning_point();let tip=source.get_sink();
    assert_ne!(point,net.config.params.genesis.hash,"campaign must reach an actual non-genesis pruning point");
    let proof=source.get_pruning_point_proof();assert_eq!(proof.first().and_then(|l|l.last()).map(|h|h.hash),Some(point));
    let mut validation_cfg=net.config.clone();validation_cfg.process_genesis=true;
    let validator=TestConsensus::new(&validation_cfg);validator.set_validation_time_for_test(net.chain.ctx.simulated_time);
    validator.validate_pruning_proof(&proof,&PruningProofMetadata::new(source.get_header(tip).unwrap().blue_work)).unwrap();drop(validator);
    let trust=source.get_pruning_point_anticone_and_trusted_data().unwrap();
    let entries=trust.anticone.iter().map(|hash|TrustedDataEntry::new(source.get_block(*hash).unwrap(),vec![],vec![])).collect();
    let package=TrustedDataPackage::new(
        trust.daa_window_blocks.iter().map(|r|TrustedHeader::new(r.header.clone(),r.ghostdag.clone())).collect(),
        trust.ghostdag_blocks.iter().map(|r|TrustedGhostdagData::new(r.hash,r.ghostdag.clone())).collect());
    let trusted=package.build_trusted_subdag(entries).unwrap();
    let mut cfg=net.config.clone();cfg.process_genesis=false;
    let target=TestConsensus::new(&cfg);target.set_validation_time_for_test(net.chain.ctx.simulated_time);
    target.apply_pruning_proof((*proof).clone(),&trusted).unwrap();target.import_pruning_points(source.pruning_point_headers()).unwrap();
    for block in trusted{target.validate_and_insert_trusted_block(block).virtual_state_task.await.unwrap();}
    target.clear_body_missing_anticone_set();headers_after_point(source,&target,point,tip).await;
    target.validate_pruning_points(tip).unwrap();
    // Same ordinary snapshot/UTXO/native/EVM session importer used by the
    // retained-source pruned experiment; complete declarations stay mandatory.
    import_snapshot(source,&target,point,&cfg);
    let missing=target.get_missing_block_body_hashes(tip).unwrap();
    for hash in missing{target.validate_and_insert_block(source.get_block(hash).unwrap()).virtual_state_task.await.unwrap();}
    assert_eq!(target.get_sink(),tip);
    let (_,left)=source.virtual_processor().palw_state_v2_store.read().load_tip(&net.bundle.state).unwrap().unwrap();
    let (_,right)=target.virtual_processor().palw_state_v2_store.read().load_tip(&net.bundle.state).unwrap().unwrap();
    assert_eq!(left.state_root(),right.state_root(),"fresh pruned importer reproduces checkpoint and material after the source's real reorg/restart");
    assert_eq!(target.palw_kernel_route_v1(),source.palw_kernel_route_v1(),"actual imported native rows match");
    assert_eq!(target.get_virtual_parents(),source.get_virtual_parents(),"imported virtual UTXO views refer to the same parents");
    let imported=target.get_virtual_utxos(None,1_000_000,false);assert!(imported.len()<1_000_000,"complete imported UTXO enumeration");
    let imported:std::collections::HashMap<_,_>=imported.into_iter().collect();
    assert_eq!(imported,Net::utxos_of(&net.chain),"fresh pruned importer reproduces every virtual UTXO");
    assert_eq!(target.get_evm_canonical_heads().unwrap(),source.get_evm_canonical_heads().unwrap(),"actual EVM Latest/Safe/Finalized heads match");
    if let Some(pending)=pending{
    use kaspa_consensus_core::palw_claim_local_v1::{PalwClaimLocalWorkRefV1 as W,PALW_CLAIM_LOCAL_TABLE_V1,palw_decode_claim_checkpoint_v1};
    let key=kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_key_v1();
    let checkpoint=palw_decode_claim_checkpoint_v1(&right.kernel_route_state_v1().unwrap().aux[&(PALW_CLAIM_LOCAL_TABLE_V1,key.to_vec())]).unwrap();
    let record=&checkpoint.records[&W::NativeMiningClaim39{claim:pending}];
    assert!(!record.certified&&!record.liability_closed&&!record.material.is_empty());
    assert!(record.material.iter().all(|id|checkpoint.material.contains_key(id)),"every bound literal material object survived actual pruned import");
    }
    assert_eq!(target.get_native_settlement_snapshot().unwrap(),source.get_native_settlement_snapshot().unwrap());
}
