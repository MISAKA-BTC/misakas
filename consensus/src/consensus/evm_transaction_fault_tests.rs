//! Actual cold raw/index/execution-header read contracts, not accepted transactions or maturity facts.
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::{
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    evm::{EvmExecutionHeader, EvmRawTx, EvmTxLocations},
};
use kaspa_database::{
    create_temp_db,
    prelude::{ConnBuilder, DbKey},
    registry::DatabaseStorePrefixes,
};
struct Native {
    node: TestConsensus,
    db: Arc<DB>,
    handles: Vec<std::thread::JoinHandle<()>>,
    _life: kaspa_database::utils::DbLifetime,
}
impl Native {
    fn new() -> Self {
        let (life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (tx, _rx) = async_channel::unbounded();
        let node = TestConsensus::with_db(db.clone(), &ConfigBuilder::new(MAINNET_PARAMS).build(), tx);
        let handles = node.init();
        Self { node, db, handles, _life: life }
    }
    fn put(&self, prefix: DatabaseStorePrefixes, key: impl Clone + AsRef<[u8]>, raw: &[u8]) {
        self.db.put(DbKey::new(&Vec::<u8>::from(prefix), key), raw).unwrap();
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.node.shutdown(std::mem::take(&mut self.handles));
    }
}
fn tx(n: u8) -> kaspa_hashes::EvmH256 {
    kaspa_hashes::EvmH256::from_bytes([n; 32])
}
#[test]
fn evm_transaction_native_cold_index_error_is_not_empty_locations() {
    let n = Native::new();
    assert_eq!(n.node.get_evm_tx_locations(tx(1)).unwrap(), EvmTxLocations::default());
    n.put(DatabaseStorePrefixes::EvmTxLookup, tx(2), &[0xff]);
    let actual = n.node.get_evm_tx_locations(tx(2));
    assert!(matches!(actual, Err(ConsensusError::GeneralOwned(_))), "{actual:?}");
    let expected = EvmTxLocations {
        included_in: vec![BlockHash::from_u64_word(3)],
        accepted_in: vec![(BlockHash::from_u64_word(4), 2)],
        last_skip_class: Some(5),
    };
    n.put(DatabaseStorePrefixes::EvmTxLookup, tx(3), &bincode::serialize(&expected).unwrap());
    assert_eq!(n.node.get_evm_tx_locations(tx(3)).unwrap(), expected);
}
#[test]
fn evm_transaction_native_cold_raw_error_is_not_unknown_or_pending_fallback() {
    let n = Native::new();
    assert!(n.node.get_evm_raw_tx(tx(4)).unwrap().is_none());
    n.put(DatabaseStorePrefixes::EvmRawTransaction, tx(5), &[0xff]);
    let actual = n.node.get_evm_raw_tx(tx(5));
    assert!(matches!(actual, Err(ConsensusError::GeneralOwned(_))), "{actual:?}");
    let expected = vec![2, 9, 8, 7];
    n.put(
        DatabaseStorePrefixes::EvmRawTransaction,
        tx(6),
        &bincode::serialize(&EvmRawTx { raw: expected.clone(), payload_block: BlockHash::from_u64_word(8) }).unwrap(),
    );
    assert_eq!(n.node.get_evm_raw_tx(tx(6)).unwrap(), Some(expected));
}

#[test]
fn evm_transaction_native_cold_execution_header_error_is_not_absent_genesis_context() {
    let n = Native::new();
    let missing = BlockHash::from_u64_word(0x12ea11);
    assert!(n.node.get_evm_header_of(missing).unwrap().is_none());
    let broken = BlockHash::from_u64_word(0x12ea12);
    n.put(DatabaseStorePrefixes::EvmHeader, broken, &[0xff]);
    let actual = n.node.get_evm_header_of(broken);
    assert!(matches!(actual, Err(ConsensusError::GeneralOwned(_))), "{actual:?}");
    let positive = BlockHash::from_u64_word(0x12ea13);
    let expected = EvmExecutionHeader { evm_number: 17, evm_timestamp_sec: 29, gas_limit: 30_000_000, ..Default::default() };
    n.put(DatabaseStorePrefixes::EvmHeader, positive, &bincode::serialize(&expected).unwrap());
    assert_eq!(n.node.get_evm_header_of(positive).unwrap(), Some(expected));
}
