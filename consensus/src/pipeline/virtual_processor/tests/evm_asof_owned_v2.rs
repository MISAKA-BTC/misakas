//! Actual native post-block state reads. No supplied claim or Final facts,
//! model examination, EVM execution or public/RPC certification is asserted.
use super::*;
use crate::{
    model::stores::{
        ghostdag::{CompactGhostdagData, GhostdagStore, GhostdagStoreReader},
        pruning::PruningStoreReader,
        pruning_meta::PruningMetaStores,
    },
    pipeline::virtual_processor::NativeEvmAsOfFaultV2,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_database::{
    prelude::{CachePolicy, DbKey},
    registry::DatabaseStorePrefixes,
};
fn context() -> TestContext {
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    TestContext::new(TestConsensus::new(&config))
}
async fn extend(ctx: &mut TestContext, parent: BlockHash, nonce: u64) -> BlockHash {
    assert_eq!(ctx.consensus.get_sink(), parent, "a real producer builds on its current accepted sink");
    eprintln!("asof extend: preparing nonce={nonce} parent={parent}");
    let t = ctx.build_block_template_keeping_time(nonce);
    assert_eq!(t.block.header.direct_parents(), &[parent]);
    let hash = t.block.header.hash;
    let status = ctx.consensus.validate_and_insert_block(t.into_immutable_v3().unwrap()).virtual_state_task.await.unwrap();
    assert_eq!(status, BlockStatus::StatusUTXOValid, "producer must accept the actual template UTXO state");
    assert_eq!(ctx.consensus.get_sink(), hash);
    eprintln!("asof extend: inserted nonce={nonce} hash={hash} status={status:?} actual_sink={}", ctx.consensus.get_sink());
    hash
}
#[tokio::test]
async fn evm_asof_native_post_target_differs_from_parent_and_survives_real_branch_switch() {
    kaspa_core::log::try_init_logger("info");
    let mut ctx = context();
    let genesis = ctx.consensus.params().genesis.hash;
    let p = extend(&mut ctx, genesis, 1).await;
    let a = extend(&mut ctx, p, 2).await;
    assert_eq!(ctx.consensus.get_sink(), a);
    let vp = ctx.consensus.virtual_processor().clone();
    let state_a = vp.palw_evm_post_state_owned_v2(a).unwrap().unwrap();
    let root_a = state_a.state_root();
    let header_a = vp.headers_store.get_header(a).unwrap();
    assert_eq!(state_a.last_point().unwrap().daa_score, header_a.daa_score);
    let view = state_a
        .evm_view_owned_v2(
            kaspa_consensus_core::evm::EVM_CHAIN_ID,
            vp.palw_state_params_v2.as_ref().unwrap().base_class_id(),
            Default::default(),
        )
        .unwrap();
    assert_eq!(view.chain_daa, header_a.daa_score);
    assert_eq!(
        &**view,
        &state_a.evm_view_v1(kaspa_consensus_core::evm::EVM_CHAIN_ID, vp.palw_state_params_v2.as_ref().unwrap().base_class_id())
    );
    drop(view);
    let state_p = vp.palw_evm_post_state_owned_v2(p).unwrap().unwrap();
    assert_eq!(state_a.last_point().unwrap().block, a);
    assert_eq!(state_p.last_point().unwrap().block, p);
    assert_ne!(root_a, state_p.state_root(), "actual after-A state must not be silently shifted to parent(A)");
    drop(state_a);
    drop(state_p);
    // A second real node builds the competing branch at its own accepted tip.
    // The arbitrary-parent TestBlockBuilder uses ORIGIN as mock finality and
    // cannot safely template a losing historical branch on the first node.
    let mut peer = context();
    assert_eq!(peer.consensus.params().consensus_params_id(), ctx.consensus.params().consensus_params_id());
    assert_eq!(peer.simulated_time, ctx.simulated_time, "both producers use the same unchanged clock setup");
    let parent_block = ctx.consensus.get_block(p).unwrap();
    peer.validate_and_insert_block(parent_block).await;
    assert_eq!(peer.consensus.get_sink(), p);
    assert_eq!(peer.consensus.block_status(p), BlockStatus::StatusUTXOValid);
    let b = extend(&mut peer, p, 3).await;
    let b1 = extend(&mut peer, b, 4).await;
    for hash in [b, b1] {
        assert_eq!(peer.consensus.block_status(hash), BlockStatus::StatusUTXOValid);
        let block = peer.consensus.get_block(hash).unwrap();
        eprintln!("asof branch: importing actual producer block={hash}");
        ctx.validate_and_insert_block(block).await;
    }
    assert_eq!(ctx.consensus.get_sink(), b1);
    for hash in [b, b1] {
        assert_eq!(ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid);
    }
    assert!(!ctx.consensus.is_chain_block(a).unwrap());
    let replay_a = vp.palw_evm_post_state_owned_v2(a).unwrap().unwrap();
    assert_eq!(replay_a.state_root(), root_a);
    assert_eq!(replay_a.last_point().unwrap().block, a);
    drop(replay_a);
    // Stop only this fixture's workers before destructive metadata/history
    // faults. The real branch acceptance and positive reads completed above.
    ctx.consensus.shutdown(std::mem::take(&mut ctx.join_handles));
    let mut batch = rocksdb::WriteBatch::default();
    vp.palw_state_v2_store.write().delete_delta_batch(&mut batch, a).unwrap();
    vp.db.write(batch).unwrap();
    assert!(matches!(vp.palw_evm_post_state_owned_v2(a), Err(NativeEvmAsOfFaultV2::HistoryUnavailable)));
    // Real reader control for a corrupt selected-parent cycle. X is an
    // unaccepted storage row, never supplied as an accepted claim/block fact.
    // The accepted target A remains fixed; only its compact metadata is faulted.
    let original = vp.ghostdag_store.get_data(a).unwrap();
    let x = BlockHash::from_bytes([0xab; 64]);
    let mut batch = rocksdb::WriteBatch::default();
    vp.ghostdag_store.delete_batch(&mut batch, a).unwrap();
    vp.db.write(batch).unwrap();
    let prefix = DatabaseStorePrefixes::GhostdagCompact.into_iter().chain(0u8.to_le_bytes()).collect::<Vec<_>>();
    for (block, parent) in [(a, x), (x, a)] {
        let row = CompactGhostdagData { blue_score: original.blue_score, blue_work: original.blue_work, selected_parent: parent };
        vp.db.put(DbKey::new(&prefix, block), bincode::serialize(&row).unwrap()).unwrap();
    }
    let cycle = vp.palw_evm_post_state_owned_v2(a);
    vp.ghostdag_store.delete(a).unwrap();
    vp.ghostdag_store.delete(x).unwrap();
    vp.ghostdag_store.insert(a, original).unwrap();
    assert!(matches!(cycle, Err(NativeEvmAsOfFaultV2::Context("cyclic selected parent"))));
}
#[tokio::test]
async fn evm_asof_native_real_import_generation_change_refuses_completed_walk() {
    let mut ctx = context();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    let vp = ctx.consensus.virtual_processor();
    let target = ctx.consensus.get_sink();
    assert!(vp.palw_evm_post_state_owned_v2(target).unwrap().is_some());
    vp.observe_evm_asof_before_recheck_v2(Some(|vp| {
        let point = vp.pruning_point_store.read().pruning_point().unwrap();
        vp.begin_pruning_point_import(point).unwrap();
    }));
    assert!(matches!(vp.palw_evm_post_state_owned_v2(target), Err(NativeEvmAsOfFaultV2::Context("import generation changed"))));
    assert!(matches!(vp.palw_evm_post_state_owned_v2(target), Err(NativeEvmAsOfFaultV2::Context("import incomplete"))));
}
#[tokio::test]
async fn evm_asof_native_cold_meta_and_reachability_errors_are_not_history_absence() {
    let mut ctx = context();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    ctx.consensus.shutdown(std::mem::take(&mut ctx.join_handles));
    let vp = ctx.consensus.virtual_processor();
    let target = ctx.consensus.get_sink();
    let missing = BlockHash::from_bytes([0xa8; 64]);
    assert!(matches!(vp.palw_evm_post_state_owned_v2(missing), Err(NativeEvmAsOfFaultV2::HistoryUnavailable)));
    let corrupt = BlockHash::from_bytes([0xa9; 64]);
    let reachability_prefix =
        DatabaseStorePrefixes::Reachability.into_iter().chain([u8::from(DatabaseStorePrefixes::Separator)]).collect::<Vec<_>>();
    vp.db.put(DbKey::new(&reachability_prefix, corrupt), [0xff]).unwrap();
    let actual = vp.palw_evm_post_state_owned_v2(corrupt);
    assert!(matches!(actual, Err(NativeEvmAsOfFaultV2::Store(_))), "malformed actual reachability row: {actual:?}");
    // A cold replacement of the same DB-backed metadata reader exposes actual
    // serialized marker failure rather than a preexisting cached true flag.
    vp.db.put(DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PruningUtxosetSyncFlag)), [0xff]).unwrap();
    *vp.pruning_meta_stores.write() = PruningMetaStores::new(vp.db.clone(), CachePolicy::Empty);
    let actual = vp.palw_evm_post_state_owned_v2(target);
    assert!(matches!(actual, Err(NativeEvmAsOfFaultV2::Store(_))), "malformed cold sync marker: {actual:?}");
}
#[tokio::test]
async fn evm_asof_native_local_capacity_is_resource_not_missing_history() {
    const CHILD: &str = "MISAKA_NATIVE_EVM_ASOF_CAPACITY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::evm_asof_native_local_capacity_is_resource_not_missing_history", module_path!());
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let mut ctx = context();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    ctx.consensus.shutdown(std::mem::take(&mut ctx.join_handles));
    use kaspa_consensus_core::palw_native_resource_v1::*;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(matches!(
        ctx.consensus.virtual_processor().palw_evm_post_state_owned_v2(ctx.consensus.get_sink()),
        Err(NativeEvmAsOfFaultV2::Resource(_))
    ));
}
