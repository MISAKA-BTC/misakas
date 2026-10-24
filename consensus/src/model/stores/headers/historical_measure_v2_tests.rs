use super::*;
use kaspa_consensus_core::{config::params::MAINNET_PARAMS, header::CompressedParents};
use kaspa_database::{create_temp_db, prelude::ConnBuilder};

fn fixture(runs: usize, commitment: usize) -> Header {
    let mut header: Header = (&MAINNET_PARAMS.genesis).into();
    let mut parents = CompressedParents::default();
    for level in 0..runs {
        let hashes = (0..(level % 5 + 1)).map(|n| BlockHash::from_u64_word((level * 10 + n) as u64)).collect();
        parents.push(hashes);
    }
    header.parents_by_level = parents;
    header.palw_commitment = vec![0xab; commitment];
    header.blue_work = kaspa_consensus_core::BlueWorkType::MAX;
    header
}

#[test]
fn current_header_measure_matches_actual_borsh_without_materialization() {
    for runs in [0, 1, 2, 31, 255] {
        for commitment in [0, 1, 4097, 16385] {
            for version in [0, 1, 2] {
                let mut header = fixture(runs, commitment);
                header.version = version;
                let expected = borsh::object_length(&header).unwrap() as u64;
                let bytes = bincode::serialize(&HeaderWithBlockLevel { header: Arc::new(header), block_level: 3 }).unwrap();
                assert_eq!(measure_current(&bytes).unwrap(), expected);
                let mut trailing = bytes.clone();
                trailing.push(0);
                assert!(measure_current(&trailing).is_err());
                assert!(measure_current(&bytes[..bytes.len() - 1]).is_err());
            }
        }
    }
}

fn legacy_header() -> HeaderWithBlockLevel2 {
    let h = fixture(0, 0);
    // Adjacent repeated levels must compress exactly as the existing shim.
    let a = vec![BlockHash::from_u64_word(1), BlockHash::from_u64_word(2)];
    HeaderWithBlockLevel2 {
        block_level: 1,
        header: Header2 {
            hash: h.hash,
            version: h.version,
            parents_by_level: vec![a.clone(), a, vec![], vec![]],
            hash_merkle_root: h.hash_merkle_root,
            accepted_id_merkle_root: h.accepted_id_merkle_root,
            utxo_commitment: h.utxo_commitment,
            timestamp: h.timestamp,
            bits: h.bits,
            nonce: h.nonce,
            daa_score: h.daa_score,
            blue_work: h.blue_work,
            blue_score: h.blue_score,
            pruning_point: h.pruning_point,
        },
    }
}

#[test]
fn actual_header_store_measure_handles_cache_cold_legacy_absence_and_corruption() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let store = DbHeadersStore::new(db.clone(), CachePolicy::Count(4), CachePolicy::Empty);
    let header = fixture(17, 4097);
    let hash = header.hash;
    let expected = borsh::object_length(&header).unwrap() as u64;
    store.insert(hash, Arc::new(header), 1).unwrap();
    assert_eq!(store.historical_header_borsh_len_v2(hash).unwrap(), expected);
    let cold = store.clone_with_new_cache(CachePolicy::Count(4), CachePolicy::Empty);
    assert!(cold.headers_access.read_from_cache(hash).is_none());
    assert_eq!(cold.historical_header_borsh_len_v2(hash).unwrap(), expected);
    assert!(cold.headers_access.read_from_cache(hash).is_none(), "measurement must not populate header cache");
    let absent = BlockHash::from_u64_word(999);
    assert!(matches!(cold.historical_header_borsh_len_v2(absent), Err(StoreError::KeyNotFound(_))));
    let legacy = legacy_header();
    let converted: HeaderWithBlockLevel = legacy.clone().into();
    let legacy_hash = BlockHash::from_u64_word(998);
    db.put(DbKey::new(&cold.fallback_prefix, legacy_hash), bincode::serialize(&legacy).unwrap()).unwrap();
    assert_eq!(
        cold.historical_header_borsh_len_v2(legacy_hash).unwrap(),
        borsh::object_length(converted.header.as_ref()).unwrap() as u64
    );
    let prefix: Vec<u8> = DatabaseStorePrefixes::CompressedHeaders.into();
    db.put(DbKey::new(&prefix, legacy_hash), [0u8]).unwrap();
    assert!(
        matches!(cold.historical_header_borsh_len_v2(legacy_hash), Err(StoreError::DeserializationError(_))),
        "a corrupt current row must not fall back to legacy"
    );
}

#[test]
fn historical_measure_capacity_isolated() {
    let worker = format!("{}::historical_measure_capacity_child", module_path!().split_once("::").unwrap().1);
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &worker, "--nocapture", "--test-threads=1"])
        .env("MISAKA_HEADER_MEASURE_CHILD_V2", "1")
        .status()
        .unwrap();
    assert!(result.success());
}

#[test]
fn historical_measure_capacity_child() {
    if std::env::var("MISAKA_HEADER_MEASURE_CHILD_V2").as_deref() != Ok("1") {
        return;
    }
    use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let store = DbHeadersStore::new(db.clone(), CachePolicy::Empty, CachePolicy::Empty);
    let header = fixture(255, 16385);
    let hash = header.hash;
    let expected = borsh::object_length(&header).unwrap() as u64;
    store.insert(hash, Arc::new(header), 1).unwrap();
    let legacy = legacy_header();
    let legacy_hash = BlockHash::from_u64_word(998);
    db.put(DbKey::new(&store.fallback_prefix, legacy_hash), bincode::serialize(&legacy).unwrap()).unwrap();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert_eq!(store.historical_header_borsh_len_v2(hash).unwrap(), expected, "no cold current header allocation needs admission");
    assert!(
        matches!(store.historical_header_borsh_len_v2(legacy_hash), Err(StoreError::LocalResourceUnavailable(_))),
        "legacy owning decode refuses before allocation"
    );
}
