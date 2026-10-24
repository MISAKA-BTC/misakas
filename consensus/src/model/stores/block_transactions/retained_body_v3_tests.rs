//! Actual DB/cache payload ownership; isolated logical quota, not RSS coverage.
use super::*;
use kaspa_consensus_core::{
    block::{BlockTemplate, MutableBlock},
    coinbase::{CoinbaseAllocationV2, MinerData},
    config::params::MAINNET_PARAMS,
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1},
    subnets::SUBNETWORK_ID_COINBASE,
    tx::{ScriptPublicKey, scriptvec},
};

#[test]
fn db_body_cache_and_external_owner_retain_render_admission_until_last_drop() {
    let worker = format!("{}::capacity_child", module_path!().split_once("::").unwrap().1);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &worker, "--nocapture", "--test-threads=1"])
        .env("MISAKA_DB_BODY_ADMISSION_CHILD_V3", "1")
        .status()
        .unwrap();
    assert!(status.success(), "DB body admission child failed: {status}");
}

#[test]
fn capacity_child() {
    if std::env::var("MISAKA_DB_BODY_ADMISSION_CHILD_V3").as_deref() != Ok("1") {
        return;
    }
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    const LIMIT: u64 = 4096;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let store = DbBlockTransactionsStore::new(db.clone(), CachePolicy::Count(4));
    let script = ScriptPublicKey::new(0, scriptvec![0x51]);
    let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_COINBASE, 0, vec![1, 2, 3]);
    let mut allocation = CoinbaseAllocationV2::default();
    allocation.grow_v2(1024).unwrap();
    let block = BlockTemplate::new(
        MutableBlock::new((&MAINNET_PARAMS.genesis).into(), vec![tx]),
        MinerData::new(script, vec![]),
        false,
        vec![],
        0,
        0,
        MAINNET_PARAMS.genesis.hash,
        vec![],
        vec![],
        vec![],
    )
    .with_coinbase_allocation_v2(allocation)
    .into_immutable_v3()
    .unwrap();
    let hash = block.hash();
    store.insert(hash, block.transactions.clone()).unwrap();
    drop(block);
    assert!(reserve_native_requests_v1(LIMIT).is_err(), "cache retains inserted render owner");
    let external = store.get(hash).unwrap();
    let another = store.get(hash).unwrap();
    assert_eq!(external[0].payload, vec![1, 2, 3]);
    drop(another);
    // A new cache decodes the actual serialized row. Local admission is not a
    // persisted capability; the decoded body does not acquire the old ticket.
    let cold_store = store.clone_with_new_cache(CachePolicy::Empty);
    let cold_body = cold_store.get(hash).unwrap();
    assert_eq!(cold_body[0].payload, external[0].payload);
    store.delete(hash).unwrap();
    assert!(!store.has(hash).unwrap());
    assert!(cold_store.get(hash).is_err());
    assert!(reserve_native_requests_v1(LIMIT).is_err(), "deletion cannot revoke an external live owner");
    drop(store);
    assert!(reserve_native_requests_v1(LIMIT).is_err(), "external body still owns its ticket after cache drop");
    drop(external);
    let restored = reserve_native_requests_v1(LIMIT).unwrap();
    assert!(restored.is_some(), "last admitted owner releases despite an unadmitted cold-decoded body");
    assert_eq!(cold_body[0].payload, vec![1, 2, 3]);
    drop(restored);
    drop(cold_body);
    drop(cold_store);
}
