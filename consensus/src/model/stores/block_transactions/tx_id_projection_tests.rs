//! Parity against the real persisted codec; no synthetic substitute codec.
use super::*;
use kaspa_consensus_core::{
    subnets::SubnetworkId,
    tx::{ScriptPublicKey, TransactionOutpoint},
};
use kaspa_database::{create_temp_db, prelude::ConnBuilder};

fn transaction(seed: u64, long: bool) -> Transaction {
    let mut tx = Transaction::new_with_mass(
        0x1234,
        vec![TransactionInput::new(
            TransactionOutpoint::new(BlockHash::from_u64_word(seed), 123),
            vec![7; if long { 7226 } else { 0 }],
            0xfedcba9876543210,
            251,
        )],
        vec![TransactionOutput::new(0xabcde, ScriptPublicKey::from_vec(0x1234, vec![3; if long { 8192 } else { 69 }]))],
        0x123456789abcdef0,
        SubnetworkId::from_byte(0xab),
        12345,
        vec![11; if long { 4 << 20 } else { 3 }],
        0x123456,
    );
    // The old getter serves the stored cached ID, even if a local test/body
    // writer deliberately made it stale. Projection must not recompute it.
    tx.payload.push(9);
    assert!(!tx.id_is_current());
    tx
}

#[test]
fn stored_codec_projection_matches_empty_long_nested_and_stale_cached_ids() {
    for txs in [vec![], vec![transaction(1, true)], vec![transaction(2, false), transaction(3, true), transaction(4, false)]] {
        let encoded = bincode::serialize(&BlockBody(txs.clone().into())).unwrap();
        let decoded: BlockBody = bincode::deserialize(&encoded).unwrap();
        let mut ids = Vec::new();
        assert_eq!(tx_id_projection::scan_ids(&encoded, |raw| ids.push(BlockHash::from_bytes(*raw))).unwrap(), txs.len());
        assert_eq!(ids, decoded.0.iter().map(Transaction::id).collect::<Vec<_>>());
        for n in [0, 1, encoded.len() / 2, encoded.len() - 1] {
            assert!(tx_id_projection::scan_ids(&encoded[..n], |_| {}).is_err());
        }
    }
}

#[test]
fn cold_projection_does_not_populate_body_cache_and_warm_ids_are_identical() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let cold = DbBlockTransactionsStore::new(db.clone(), CachePolicy::Count(0));
    let warm = DbBlockTransactionsStore::new(db, CachePolicy::Count(4));
    let hash = BlockHash::from_u64_word(999);
    let transactions = vec![transaction(1, true), transaction(2, false)];
    let expected = transactions.iter().map(Transaction::id).collect::<Vec<_>>();
    cold.insert(hash, transactions.into()).unwrap();
    assert!(cold.access.read_from_cache(hash).is_none());
    let projected = cold.rpc_transaction_ids_v1(hash).unwrap();
    assert_eq!(projected.ids, expected);
    assert!(projected.memory.is_some());
    assert!(cold.access.read_from_cache(hash).is_none(), "ID projection must not materialize/cache the cold UTXO body");
    let full = warm.get(hash).unwrap();
    assert!(warm.access.read_from_cache(hash).is_some());
    assert_eq!(warm.rpc_transaction_ids_v1(hash).unwrap().ids, projected.ids);
    assert_eq!(full.iter().map(Transaction::id).collect::<Vec<_>>(), expected);
}

#[test]
fn malformed_row_is_a_store_error_and_missing_is_not_an_empty_body() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let store = DbBlockTransactionsStore::new(db.clone(), CachePolicy::Count(0));
    let hash = BlockHash::from_u64_word(123);
    assert!(matches!(store.rpc_transaction_ids_v1(hash), Err(StoreError::KeyNotFound(_))));
    let prefix: Vec<u8> = DatabaseStorePrefixes::BlockTransactions.into();
    let key = kaspa_database::prelude::DbKey::new(&prefix, hash);
    db.put(key, u64::MAX.to_le_bytes()).unwrap();
    assert!(matches!(store.rpc_transaction_ids_v1(hash), Err(StoreError::DataInconsistency(_))));
}

#[test]
fn trailing_bytes_match_real_default_bincode_and_both_store_paths() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let txs = vec![transaction(7, false), transaction(8, false)];
    let mut bytes = bincode::serialize(&BlockBody(txs.into())).unwrap();
    bytes.extend([0xff; 1024]);
    // This is the actual configuration CachedDbAccess::read uses, rather
    // than DefaultOptions::new(), whose trailing-byte policy is different.
    let decoded: BlockBody = bincode::deserialize(&bytes).unwrap();
    let expected = decoded.0.iter().map(Transaction::id).collect::<Vec<_>>();
    let hash = BlockHash::from_u64_word(456);
    let prefix: Vec<u8> = DatabaseStorePrefixes::BlockTransactions.into();
    db.put(kaspa_database::prelude::DbKey::new(&prefix, hash), &bytes).unwrap();
    let cold = DbBlockTransactionsStore::new(db.clone(), CachePolicy::Count(0));
    let warm = DbBlockTransactionsStore::new(db, CachePolicy::Count(4));
    assert_eq!(cold.rpc_transaction_ids_v1(hash).unwrap().ids, expected);
    assert_eq!(warm.get(hash).unwrap().iter().map(Transaction::id).collect::<Vec<_>>(), expected);
    assert_eq!(warm.rpc_transaction_ids_v1(hash).unwrap().ids, expected);
}
