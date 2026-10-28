//! Actual native/store guard controls. No pruning-depth override or accepted
//! claim/Final facts are supplied by these snapshot reader fixtures.
#![allow(warnings)]
use super::*;
use crate::{model::stores::block_transactions::BlockTransactionsStoreReader, pipeline::virtual_processor::PalwPruningCaptureFaultV1};
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
struct RestoreRow {
    db: std::sync::Arc<kaspa_database::prelude::DB>,
    key: DbKey,
    old: Vec<u8>,
}
impl RestoreRow {
    fn corrupt(db: std::sync::Arc<kaspa_database::prelude::DB>, key: DbKey) -> Self {
        let old = db.get(&key).unwrap().unwrap();
        db.put(&key, [0xff]).unwrap();
        Self { db, key, old }
    }
}
impl Drop for RestoreRow {
    fn drop(&mut self) {
        self.db.put(&self.key, &self.old).unwrap();
    }
}

#[tokio::test]
async fn pruning_snapshot_readiness_fault_preserves_body_delta_and_same_point_repairs() {
    let mut ctx = context();
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor();
    let point = ctx.consensus.get_sink();
    let before_body = vp.block_transactions_store.get(point).unwrap();
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    let tip_key = DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PalwStateV2Tip));
    let fault = RestoreRow::corrupt(vp.db.clone(), tip_key);
    let entered = std::cell::Cell::new(0);
    let refusal = vp.with_pruning_snapshots_before_delete_v1(point, || {
        entered.set(1);
    });
    assert!(matches!(refusal, Err(PalwPruningCaptureFaultV1::Store(_))));
    assert_eq!(entered.get(), 0);
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert_eq!(vp.block_transactions_store.get(point).unwrap().len(), before_body.len());
    drop(fault);
    vp.with_pruning_snapshots_before_delete_v1(point, || {
        entered.set(entered.get() + 1);
        let mut batch = rocksdb::WriteBatch::default();
        let mut palw = vp.palw_state_v2_store.write();
        palw.delete_delta_batch(&mut batch, point).unwrap();
        vp.block_transactions_store.delete_batch(&mut batch, point).unwrap();
        vp.db.write(batch).unwrap();
    })
    .unwrap();
    assert_eq!(entered.get(), 1, "same-point retry enters deletion exactly once after repair");
    assert!(!vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert!(vp.block_transactions_store.get(point).is_err());
    assert_eq!(ctx.consensus.pruning_point_palw_state_shared_v1(point).unwrap().last_point.as_ref().unwrap().block, point);
}

#[tokio::test]
async fn pruning_snapshot_readiness_reuses_cold_exact_snapshot_after_partial_prune() {
    let mut ctx = context();
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor();
    let point = ctx.consensus.get_sink();
    vp.capture_pruning_point_palw_state(point).unwrap();
    let record = vp.palw_state_v2_store.read().pruning_snapshot_record().unwrap().unwrap();
    let mut batch = rocksdb::WriteBatch::default();
    vp.palw_state_v2_store.write().delete_delta_batch(&mut batch, point).unwrap();
    vp.db.write(batch).unwrap();
    let old_store = vp.palw_state_v2_store.read().clone_with_new_cache(CachePolicy::Empty);
    *vp.palw_state_v2_store.write() = old_store;
    // Recovery must use the validated exact snapshot even if the old tip source
    // cannot now be decoded. The restored tip fault is unrelated to that snapshot.
    let tip_key = DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PalwStateV2Tip));
    let fault = RestoreRow::corrupt(vp.db.clone(), tip_key);
    vp.with_pruning_snapshots_before_delete_v1(point, || ()).unwrap();
    let served = ctx.consensus.pruning_point_palw_state_shared_v1(point).unwrap();
    assert_eq!(served.last_point.as_ref().unwrap().block, point);
    assert_eq!(vp.palw_state_v2_store.read().pruning_snapshot_record().unwrap().unwrap().state_root, record.state_root);
    assert!(!vp.palw_state_v2_store.read().has_delta(point).unwrap());
    drop(fault);
}

#[tokio::test]
async fn pruning_snapshot_readiness_valid_genesis_and_inactive_lane_keep_contract() {
    let ctx = context();
    let genesis = ctx.consensus.params().genesis.hash;
    let vp = ctx.consensus.virtual_processor();
    vp.capture_pruning_point_palw_state(genesis).unwrap();
    let served = ctx.consensus.pruning_point_palw_state_shared_v1(genesis).unwrap();
    assert!(served.last_point.as_ref().is_none_or(|p| p.block == genesis));
    let config = ConfigBuilder::new(MAINNET_PARAMS).build();
    let inactive = TestContext::new(TestConsensus::new(&config));
    inactive.consensus.virtual_processor().capture_pruning_point_palw_state(inactive.consensus.get_sink()).unwrap();
}

#[tokio::test]
async fn pruning_snapshot_readiness_missing_walk_history_never_enters_delete() {
    let mut ctx = context();
    let mut points = Vec::new();
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        points.push(ctx.consensus.get_sink());
    }
    let vp = ctx.consensus.virtual_processor();
    let point = points[0];
    // A genuine missing rollback delta, rather than an empty substitute. Only
    // the newest delta is removed; the protected point/body must remain intact.
    let mut batch = rocksdb::WriteBatch::default();
    vp.palw_state_v2_store.write().delete_delta_batch(&mut batch, points[2]).unwrap();
    vp.db.write(batch).unwrap();
    let entered = std::cell::Cell::new(false);
    let refusal = vp.with_pruning_snapshots_before_delete_v1(point, || entered.set(true));
    assert!(matches!(refusal, Err(PalwPruningCaptureFaultV1::Walk(
        crate::processes::palw_state_walk::PalwStateWalkError::MissingDelta(hash)
    )) if hash == points[2]));
    assert!(!entered.get());
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert!(vp.block_transactions_store.get(point).is_ok());
}

#[tokio::test]
async fn pruning_snapshot_readiness_missing_active_tip_is_context_not_inactive_success() {
    let mut ctx = context();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    let vp = ctx.consensus.virtual_processor();
    let point = ctx.consensus.get_sink();
    let key = DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PalwStateV2Tip));
    let old = vp.db.get(&key).unwrap().unwrap();
    let restore = RestoreRow { db: vp.db.clone(), key, old };
    vp.db.delete(&restore.key).unwrap();
    let entered = std::cell::Cell::new(false);
    assert!(matches!(
        vp.with_pruning_snapshots_before_delete_v1(point, || entered.set(true)),
        Err(PalwPruningCaptureFaultV1::Context("active PALW tip is absent"))
    ));
    assert!(!entered.get());
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert!(vp.block_transactions_store.get(point).is_ok());
    drop(restore);
}
