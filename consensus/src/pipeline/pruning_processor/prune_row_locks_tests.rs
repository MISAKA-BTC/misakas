//! The actual production four-store order, on real RocksDB stores. This bounded
//! child cannot strand a deadlocked thread in the unit-test process on regression.
use super::*;
use crate::model::stores::statuses::{StatusesStoreBatchExtensions, StatusesStoreReader};
use kaspa_consensus_core::{
    BlockHash,
    blockstatus::BlockStatus::{StatusHeaderOnly, StatusUTXOValid},
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_prefix_family_court_v1::PalwPrefixCourtActionV1,
    palw_prefix_history_v1::{PalwPrefixHistoryRecordV1, PalwPrefixHistoryRequestV1},
    palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2, PalwStateDeltaV2},
    tx::{TransactionId, TransactionOutpoint},
};
use kaspa_database::{
    create_temp_db,
    prelude::{CachePolicy, ConnBuilder, DbKey},
    registry::DatabaseStorePrefixes,
};
use rocksdb::WriteBatch;
use std::{
    process::{Command, Stdio},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

fn hash(n: u64) -> BlockHash {
    BlockHash::from_u64_word(n)
}
fn delta(block: BlockHash) -> PalwStateDeltaV2 {
    PalwStateDeltaV2 { point: PalwBlockContextV2 { block, daa_score: 1, blue_score: 1, subsidy: 0 }, entries: vec![] }
}
fn archive_record() -> PalwPrefixHistoryRecordV1 {
    PalwPrefixHistoryRecordV1::new(
        hash(0),
        1,
        hash(2),
        100,
        200,
        PalwConstraintStreamMoveV1 {
            version: 1,
            claim: hash(3),
            reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(4), index: 0 }),
            reserved_daa: 90,
            seed_id: hash(5),
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action: PalwConstraintStreamActionV1::Prefix(PalwPrefixCourtActionV1::FinishTile),
            signature: vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        },
    )
    .unwrap()
}
fn key(prefix: DatabaseStorePrefixes, block: BlockHash) -> DbKey {
    DbKey::new(&Vec::<u8>::from(prefix), block)
}

#[test]
fn pruning_row_lock_real_stores_complete_virtual_batch_before_pruner() {
    const CHILD: &str = "MISAKA_PRUNING_ROW_LOCK_CHILD_V1";
    if std::env::var_os(CHILD).is_none() {
        let exact = "pipeline::pruning_processor::processor::prune_row_locks::tests::pruning_row_lock_real_stores_complete_virtual_batch_before_pruner";
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", exact, "--nocapture"])
            .env(CHILD, "1")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "actual store-lock child failed: {status}");
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("owned lock-regression child timed out; production order may deadlock");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        return;
    }
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let storage = ConsensusStorage::new(db.clone(), Arc::new(ConfigBuilder::new(MAINNET_PARAMS).build()));
    let (pruned, virtual_block) = (hash(10), hash(11));
    let history = archive_record();
    let history_request = PalwPrefixHistoryRequestV1 {
        claim: history.movement.claim,
        owner: history.movement.reserver,
        reserved_daa: history.movement.reserved_daa,
        seed_id: history.movement.seed_id,
        head: history.head_v1().unwrap(),
        sequence: history.sequence,
        material_until_daa: history.material_until_daa,
    };
    let mut batch = WriteBatch::default();
    storage.palw_state_v2_store.write().insert_delta_batch(&mut batch, pruned, hash(12), &delta(pruned)).unwrap();
    storage.palw_state_v2_store.read().insert_prefix_history_record_batch_v1(&mut batch, &history).unwrap();
    storage.statuses_store.write().set_batch(&mut batch, pruned, StatusUTXOValid).unwrap();
    db.write(batch).unwrap();
    assert!(storage.palw_state_v2_store.read().prefix_history_record_v1(&history_request).unwrap().is_some());
    let durable_delta = key(DatabaseStorePrefixes::PalwStateV2Deltas, pruned);
    let durable_status = key(DatabaseStorePrefixes::Statuses, pruned);
    let before_status = db.get(&durable_status).unwrap();

    // This is the same real PALW exclusion retained by a native virtual commit.
    let mut virtual_palw = storage.palw_state_v2_store.write();
    let (arrived_tx, arrived_rx) = mpsc::sync_channel(0);
    let (allow_lock_tx, allow_lock_rx) = mpsc::sync_channel(0);
    let (staged_tx, staged_rx) = mpsc::sync_channel(0);
    let (flush_tx, flush_rx) = mpsc::sync_channel(0);
    let pruning_storage = storage.clone();
    let pruning_db = db.clone();
    let pruning = std::thread::spawn(move || {
        let RowGuards { mut palw, relations, reachability_relations, mut statuses } = lock_rows_after(&pruning_storage, || {
            arrived_tx.send(()).unwrap();
            allow_lock_rx.recv().unwrap();
        });
        let mut batch = WriteBatch::default();
        palw.delete_delta_batch(&mut batch, pruned).unwrap();
        assert_eq!(palw.gc_prefix_history_batch_v1(&mut batch, 201, 64).unwrap(), 1);
        statuses.set_batch(&mut batch, pruned, StatusHeaderOnly).unwrap();
        staged_tx.send(()).unwrap();
        flush_rx.recv().unwrap();
        pruning_db.write(batch).unwrap();
        // Keep all actual guards until the atomic row batch is durable.
        drop(statuses);
        drop(reachability_relations);
        drop(relations);
        drop(palw);
    });
    arrived_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // If production ever acquires statuses before PALW, this exact point fails
    // deterministically, before asking a blocked virtual commit to complete.
    assert!(storage.statuses_store.try_write().is_some(), "pruner acquired statuses before its first PALW guard");
    let mut virtual_batch = WriteBatch::default();
    virtual_palw.insert_delta_batch(&mut virtual_batch, virtual_block, hash(13), &delta(virtual_block)).unwrap();
    let virtual_status = storage.statuses_store.set_batch(&mut virtual_batch, virtual_block, StatusUTXOValid).unwrap();
    db.write(virtual_batch).unwrap();
    assert!(storage.palw_state_v2_store.try_write().is_none(), "virtual archive exclusion must remain held through status commit");
    drop(virtual_status);
    // Also pin the header processor's relations -> statuses order. Once the
    // pruner owns PALW but waits for these actual relations, statuses stays free.
    let held_relations = storage.relations_store.write();
    allow_lock_tx.send(()).unwrap();
    drop(virtual_palw);
    let relation_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if storage.palw_state_v2_store.try_write().is_none() {
            break;
        }
        assert!(Instant::now() < relation_deadline, "pruner never acquired its first PALW guard");
        std::thread::yield_now();
    }
    assert!(storage.statuses_store.try_write().is_some(), "pruner must not acquire statuses before relations");
    drop(held_relations);
    staged_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(storage.palw_state_v2_store.try_write().is_none());
    assert!(storage.relations_store.try_write().is_none());
    assert!(storage.reachability_relations_store.try_write().is_none());
    assert!(storage.statuses_store.try_write().is_none());
    assert!(db.get(&durable_delta).unwrap().is_some(), "staging must not delete durable history early");
    assert_eq!(db.get(&durable_status).unwrap(), before_status, "status and delta publish in one batch");
    flush_tx.send(()).unwrap();
    pruning.join().unwrap();
    assert!(storage.palw_state_v2_store.try_write().is_some());
    assert!(storage.statuses_store.try_write().is_some());
    let cold_palw = crate::model::stores::palw_state_v2::DbPalwStateV2Store::new(db.clone(), CachePolicy::Empty);
    let cold_status = crate::model::stores::statuses::DbStatusesStore::new(db.clone(), CachePolicy::Empty);
    assert!(!cold_palw.has_delta(pruned).unwrap());
    assert!(cold_palw.has_delta(virtual_block).unwrap());
    assert_eq!(cold_status.get(pruned).unwrap(), StatusHeaderOnly);
    assert_eq!(cold_status.get(virtual_block).unwrap(), StatusUTXOValid);
    assert!(
        cold_palw.prefix_history_record_v1(&history_request).unwrap().is_none(),
        "expired archive body is durably deleted with its expiry row"
    );
    let mut no_expiry_left = WriteBatch::default();
    assert_eq!(cold_palw.gc_prefix_history_batch_v1(&mut no_expiry_left, 201, 64).unwrap(), 0);
    // Cold store readers above do not reuse either writer's caches; this is not a DB restart campaign.
}
