//! Real DB codec failures at the production EVM reader API. Store fixtures,
//! not EVM execution or certified-head/campaign evidence.
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::{
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    evm::{EvmBlockReceipts, EvmExecutionHeader, EvmRawTx},
};
use kaspa_database::{
    create_temp_db,
    prelude::{ConnBuilder, DbKey},
    registry::DatabaseStorePrefixes,
};
use serde::Serialize;

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
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let node = TestConsensus::with_db(db.clone(), &config, tx);
        let handles = node.init();
        Self { node, db, handles, _life: life }
    }
    fn put(&self, prefix: DatabaseStorePrefixes, key: impl Clone + AsRef<[u8]>, bytes: &[u8]) {
        self.db.put(DbKey::new(&Vec::<u8>::from(prefix), key), bytes).unwrap();
    }
    fn row(&self, prefix: DatabaseStorePrefixes, key: impl Clone + AsRef<[u8]>, row: &impl Serialize) {
        self.put(prefix, key, &bincode::serialize(row).unwrap());
    }
    fn header(&self, block: BlockHash, number: u64) {
        self.row(DatabaseStorePrefixes::EvmHeader, block, &EvmExecutionHeader { evm_number: number, ..Default::default() });
    }
    fn number(&self, n: u64, block: BlockHash) {
        self.row(DatabaseStorePrefixes::EvmNumberIndex, number_key(n), &block);
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.node.shutdown(std::mem::take(&mut self.handles));
    }
}
fn hash(n: u8) -> BlockHash {
    BlockHash::from_bytes([n; 64])
}
fn rpc(n: u8) -> kaspa_hashes::EvmH256 {
    kaspa_hashes::EvmH256::from_bytes([n; 32])
}
fn number_key(n: u64) -> [u8; 32] {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&n.to_be_bytes());
    key
}
fn read_error<T: std::fmt::Debug>(result: ConsensusResult<T>) {
    assert!(matches!(result, Err(ConsensusError::GeneralOwned(_))), "expected typed store read fault, got {result:?}");
}

#[test]
fn evm_native_reader_cold_index_codec_errors_are_not_absence() {
    let n = Native::new();
    assert!(n.node.get_evm_block_by_number(42).unwrap().is_none());
    assert!(n.node.get_evm_block_by_rpc_hash(rpc(1)).unwrap().is_none());
    n.put(DatabaseStorePrefixes::EvmNumberIndex, number_key(42), &[0xff]);
    n.put(DatabaseStorePrefixes::EvmBlockHashMap, rpc(1), &[0xff]);
    read_error(n.node.get_evm_block_by_number(42));
    read_error(n.node.get_evm_block_by_rpc_hash(rpc(1)));
    let genesis = n.node.params().genesis.hash;
    n.number(43, genesis);
    n.header(genesis, 43);
    let block = n.node.get_evm_block_by_number(43).unwrap().unwrap();
    assert_eq!((block.l1_hash, block.header.evm_number, block.encoded_size), (genesis, 43, 0));
    assert!(block.tx_hashes.is_empty(), "absent receipts remain permitted");
    n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(2), &genesis);
    assert_eq!(n.node.get_evm_block_by_rpc_hash(rpc(2)).unwrap().unwrap(), block);
}

#[test]
fn evm_native_reader_cold_body_codec_errors_preserve_optional_rows() {
    let n = Native::new();
    assert!(n.node.get_evm_block_by_l1_hash(hash(10)).unwrap().is_none());
    n.put(DatabaseStorePrefixes::EvmHeader, hash(10), &[0xff]);
    read_error(n.node.get_evm_block_by_l1_hash(hash(10)));
    n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(10), &hash(10));
    read_error(n.node.get_evm_block_by_rpc_hash(rpc(10)));
    n.header(hash(11), 11);
    n.put(DatabaseStorePrefixes::EvmReceipts, hash(11), &[0xff]);
    read_error(n.node.get_evm_block_by_l1_hash(hash(11)));
    for id in [12, 13] {
        n.header(hash(id), id as u64);
        n.row(DatabaseStorePrefixes::EvmReceipts, hash(id), &EvmBlockReceipts { tx_hashes: vec![rpc(id)], ..Default::default() });
    }
    n.put(DatabaseStorePrefixes::EvmRawTransaction, rpc(12), &[0xff]);
    read_error(n.node.get_evm_block_by_l1_hash(hash(12)));
    let absent_raw = n.node.get_evm_block_by_l1_hash(hash(13)).unwrap().unwrap();
    assert_eq!(absent_raw.encoded_size, 0, "absent raw row keeps existing zero contribution");
    n.header(hash(14), 14);
    n.row(DatabaseStorePrefixes::EvmReceipts, hash(14), &EvmBlockReceipts { tx_hashes: vec![rpc(14)], ..Default::default() });
    n.row(DatabaseStorePrefixes::EvmRawTransaction, rpc(14), &EvmRawTx { raw: vec![1, 2, 3], payload_block: hash(14) });
    assert_eq!(n.node.get_evm_block_by_l1_hash(hash(14)).unwrap().unwrap().encoded_size, 3);
}

#[tokio::test]
async fn evm_native_reader_canonical_unknown_is_error_but_real_side_branch_is_absent() {
    let n = Native::new();
    n.number(50, hash(50));
    assert!(matches!(n.node.get_evm_block_by_number(50), Err(ConsensusError::HeaderNotFound(_))));
    let genesis = n.node.params().genesis.hash;
    for id in [60, 61] {
        n.node.add_empty_utxo_valid_block_with_parents(hash(id), vec![genesis]).await.unwrap();
    }
    let side = [hash(60), hash(61)]
        .into_iter()
        .find(|h| !n.node.is_chain_block(*h).unwrap())
        .expect("a real sibling is outside selected-parent chain");
    n.number(51, side);
    n.header(side, 51);
    assert!(n.node.get_evm_block_by_number(51).unwrap().is_none());
    n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(51), &side);
    assert_eq!(
        n.node.get_evm_block_by_rpc_hash(rpc(51)).unwrap().unwrap().l1_hash,
        side,
        "hash reader keeps historical side-branch compatibility"
    );
    // The number index still refuses a genuine header-number mismatch.
    let tip = n.node.get_sink();
    n.number(52, tip);
    n.header(tip, 999);
    assert!(n.node.get_evm_block_by_number(52).unwrap().is_none());
}
