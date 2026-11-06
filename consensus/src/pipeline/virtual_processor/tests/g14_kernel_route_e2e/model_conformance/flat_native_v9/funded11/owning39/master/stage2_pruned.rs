//! Real pruning-proof/trusted-anticone/UTXO/PALW import for the experimental
//! stage2 candidate. No genesis body replay or pruning-depth replacement.
use super::*;
use crate::model::services::reachability::ReachabilityService;
use kaspa_consensus_core::{
    pruning::PruningProofMetadata,
    trusted::{TrustedGhostdagData, TrustedHeader},
};
use kaspa_p2p_lib::convert::model::trusted::{TrustedDataEntry, TrustedDataPackage};
pub(super) async fn exercise(net: &Net, pending: Option<Hash64>) {
    exercise_with_family_carriers(net, pending, &[]).await;
}
/// The joint caller passes the independently retained accepted transactions;
/// native-only callers keep their existing empty family scope.
pub(super) async fn exercise_with_family_carriers(
    net: &Net,
    pending: Option<Hash64>,
    family: &[&family_legacy_campaign::AdmittedFamilyOriginalV2],
) {
    exercise_family_v3(net, pending, family, false).await;
}
pub(super) async fn exercise_with_closed_family_carriers(
    net: &Net,
    pending: Option<Hash64>,
    family: &[&family_legacy_campaign::AdmittedFamilyOriginalV2],
) {
    exercise_family_v3(net, pending, family, true).await;
}
async fn exercise_family_v3(
    net: &Net,
    pending: Option<Hash64>,
    family: &[&family_legacy_campaign::AdmittedFamilyOriginalV2],
    closed: bool,
) {
    use crate::pipeline::virtual_processor::tests::g14_kernel_route_e2e::material_typed_ingress_v4::source_manifest::pruned_import::{
        headers_after_point, import_snapshot,
    };
    let source = &net.chain.ctx.consensus;
    let point = source.pruning_point();
    let tip = source.get_sink();
    assert_ne!(point, net.config.params.genesis.hash, "campaign must reach an actual non-genesis pruning point");
    let proof = source.get_pruning_point_proof();
    assert_eq!(proof.first().and_then(|l| l.last()).map(|h| h.hash), Some(point));
    let mut validation_cfg = net.config.clone();
    validation_cfg.process_genesis = true;
    let validator = TestConsensus::new(&validation_cfg);
    validator.set_validation_time_for_test(net.chain.ctx.simulated_time);
    validator.validate_pruning_proof(&proof, &PruningProofMetadata::new(source.get_header(tip).unwrap().blue_work)).unwrap();
    drop(validator);
    let trust = source.get_pruning_point_anticone_and_trusted_data().unwrap();
    let entries = trust.anticone.iter().map(|hash| TrustedDataEntry::new(source.get_block(*hash).unwrap(), vec![], vec![])).collect();
    let package = TrustedDataPackage::new(
        trust.daa_window_blocks.iter().map(|r| TrustedHeader::new(r.header.clone(), r.ghostdag.clone())).collect(),
        trust.ghostdag_blocks.iter().map(|r| TrustedGhostdagData::new(r.hash, r.ghostdag.clone())).collect(),
    );
    let trusted = package.build_trusted_subdag(entries).unwrap();
    let mut cfg = net.config.clone();
    cfg.process_genesis = false;
    let target = TestConsensus::new(&cfg);
    target.set_validation_time_for_test(net.chain.ctx.simulated_time);
    target.apply_pruning_proof((*proof).clone(), &trusted).unwrap();
    target.import_pruning_points(source.pruning_point_headers()).unwrap();
    for block in trusted {
        target.validate_and_insert_trusted_block(block).virtual_state_task.await.unwrap();
    }
    target.clear_body_missing_anticone_set();
    headers_after_point(source, &target, point, tip).await;
    target.validate_pruning_points(tip).unwrap();
    // Same ordinary snapshot/UTXO/native/EVM session importer used by the
    // retained-source pruned experiment; complete declarations stay mandatory.
    import_snapshot(source, &target, point, &cfg);
    let missing = target.get_missing_block_body_hashes(tip).unwrap();
    for hash in missing {
        target.validate_and_insert_block(source.get_block(hash).unwrap()).virtual_state_task.await.unwrap();
    }
    assert_eq!(target.get_sink(), tip);
    let (_, left) = source.virtual_processor().palw_state_v2_store.read().load_tip(&net.bundle.state).unwrap().unwrap();
    let (_, right) = target.virtual_processor().palw_state_v2_store.read().load_tip(&net.bundle.state).unwrap().unwrap();
    assert_eq!(
        left.state_root(),
        right.state_root(),
        "fresh pruned importer reproduces checkpoint and material after the source's real reorg/restart"
    );
    for admitted in family {
        family_legacy_campaign::assert_family_winning_carrier_v2(&left, admitted);
        family_legacy_campaign::assert_family_winning_carrier_v2(&right, admitted);
        if closed {
            family_joint_campaign::assert_no_source_export_v3(&left, admitted);
            family_joint_campaign::assert_no_source_export_v3(&right, admitted);
        }
    }
    assert_eq!(target.palw_kernel_route_v1(), source.palw_kernel_route_v1(), "actual imported native rows match");
    assert_eq!(target.get_virtual_parents(), source.get_virtual_parents(), "imported virtual UTXO views refer to the same parents");
    let imported = target.get_virtual_utxos(None, 1_000_000, false);
    assert!(imported.len() < 1_000_000, "complete imported UTXO enumeration");
    let imported: std::collections::HashMap<_, _> = imported.into_iter().collect();
    assert_eq!(imported, Net::utxos_of(&net.chain), "fresh pruned importer reproduces every virtual UTXO");
    assert_eq!(
        target.get_evm_canonical_heads().unwrap(),
        source.get_evm_canonical_heads().unwrap(),
        "actual EVM Latest/Safe/Finalized heads match"
    );
    if let Some(pending) = pending {
        use kaspa_consensus_core::palw_claim_local_v1::PalwClaimLocalWorkRefV1 as W;
        let _validated = kaspa_consensus_core::palw_state_v2::palw_claim_local_owned_checkpoint_v1(
            &right,
            &net.bundle.state,
            right.last_point().unwrap(),
        )
        .unwrap()
        .expect("the actual imported checkpoint validates before reading its records");
        let checkpoint = kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_snapshot_v2(&right)
            .unwrap()
            .expect("the imported checkpoint has a logical snapshot");
        let record = &checkpoint.records[&W::NativeMiningClaim39 { claim: pending }];
        assert!(!record.certified && !record.liability_closed && !record.material.is_empty());
        assert!(
            record.material.iter().all(|id| checkpoint.material.contains_key(id)),
            "every bound literal material object survived actual pruned import"
        );
    }
    let target_snapshot = target.get_native_settlement_snapshot().unwrap().unwrap();
    let source_snapshot = source.get_native_settlement_snapshot().unwrap().unwrap();
    assert_eq!(target_snapshot, source_snapshot);
    let archive = kaspa_consensus_core::palw_native39_settlement_checkpoint_v1::read_native39_settlement_checkpoint_v1(&right)
        .unwrap()
        .expect("actual imported closed-prefix certificate");
    if target.virtual_processor().reachability_service.try_is_chain_ancestor_of(point, archive.effect.block).unwrap() {
        assert!(
            target_snapshot.safe.is_some() && target_snapshot.finalized.is_some(),
            "covered archival execution retains positive Safe/Finalized on a fresh pruned join"
        );
    }
    eprintln!(
        "[n39] fresh pruned join PP {point}, archive effect {}, safe {:?}, finalized {:?}; absent labels mean convergence only",
        archive.effect.block, target_snapshot.safe, target_snapshot.finalized
    );
}
