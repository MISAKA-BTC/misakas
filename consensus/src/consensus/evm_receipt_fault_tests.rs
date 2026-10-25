//! Actual production receipt reader versus cold DB faults. The receipt/index
//! rows are storage fixtures, not fabricated EVM execution or bridge payment.
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::{
    blockstatus::BlockStatus::StatusUTXOValid,
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    evm::{EvmBlockReceipts, EvmExecutionHeader, EvmLog, EvmReceipt, EvmTxLocations},
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
        let node = TestConsensus::with_db(db.clone(), &ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build(), tx);
        let handles = node.init();
        Self { node, db, handles, _life: life }
    }
    fn put(&self, prefix: DatabaseStorePrefixes, key: impl Clone + AsRef<[u8]>, bytes: &[u8]) {
        self.db.put(DbKey::new(&Vec::<u8>::from(prefix), key), bytes).unwrap();
    }
    fn row(&self, prefix: DatabaseStorePrefixes, key: impl Clone + AsRef<[u8]>, value: &impl Serialize) {
        self.put(prefix, key, &bincode::serialize(value).unwrap());
    }
    fn index(&self, tx: kaspa_hashes::EvmH256, accepted: Vec<(BlockHash, u32)>) {
        self.row(DatabaseStorePrefixes::EvmTxLookup, tx, &EvmTxLocations { accepted_in: accepted, ..Default::default() });
    }
    fn receipt(&self, block: BlockHash, tx: kaspa_hashes::EvmH256, number: u64) {
        self.row(
            DatabaseStorePrefixes::EvmReceipts,
            block,
            &EvmBlockReceipts {
                tx_hashes: vec![tx],
                receipts: vec![EvmReceipt { succeeded: true, gas_used: 21_123, ..Default::default() }],
            },
        );
        self.row(DatabaseStorePrefixes::EvmHeader, block, &EvmExecutionHeader { evm_number: number, ..Default::default() });
    }
    fn stop(&mut self) {
        self.node.shutdown(std::mem::take(&mut self.handles));
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.stop();
    }
}
fn hash(id: u64) -> BlockHash {
    BlockHash::from_u64_word(id)
}
fn tx(id: u8) -> kaspa_hashes::EvmH256 {
    kaspa_hashes::EvmH256::from_bytes([id; 32])
}
fn read_fault<T: std::fmt::Debug>(result: ConsensusResult<T>) {
    assert!(matches!(result, Err(ConsensusError::GeneralOwned(_))), "expected local read error, got {result:?}");
}
#[test]
fn evm_receipt_native_cold_index_error_is_not_genuine_absence() {
    let n = Native::new();
    assert!(n.node.get_evm_tx_receipt(tx(1)).unwrap().is_none());
    n.put(DatabaseStorePrefixes::EvmTxLookup, tx(2), &[0xff]);
    read_fault(n.node.get_evm_tx_receipt(tx(2)));
    n.row(DatabaseStorePrefixes::EvmTxLookup, tx(3), &EvmTxLocations { included_in: vec![hash(1)], ..Default::default() });
    assert!(n.node.get_evm_tx_receipt(tx(3)).unwrap().is_none());
}
#[tokio::test]
async fn evm_receipt_native_cold_receipt_and_header_errors_do_not_fall_back_to_older_receipt() {
    for broken_receipt in [true, false] {
        let mut n = Native::new();
        let genesis = n.node.params().genesis.hash;
        let ancestor = hash(if broken_receipt { 0x12ec10 } else { 0x12ec20 });
        n.node.add_empty_utxo_valid_block_with_parents(ancestor, vec![genesis]).await.unwrap();
        n.node
            .add_empty_utxo_valid_block_with_parents(hash(if broken_receipt { 0x12ec11 } else { 0x12ec21 }), vec![ancestor])
            .await
            .unwrap();
        assert!(n.node.is_chain_block(ancestor).unwrap());
        assert_ne!(ancestor, n.node.get_sink());
        n.stop();
        n.receipt(genesis, tx(4), 10);
        if broken_receipt {
            n.put(DatabaseStorePrefixes::EvmReceipts, ancestor, &[0xff]);
        } else {
            n.row(
                DatabaseStorePrefixes::EvmReceipts,
                ancestor,
                &EvmBlockReceipts { tx_hashes: vec![tx(4)], receipts: vec![EvmReceipt::default()] },
            );
            n.put(DatabaseStorePrefixes::EvmHeader, ancestor, &[0xff]);
        }
        n.index(tx(4), vec![(genesis, 0), (ancestor, 0)]);
        read_fault(n.node.get_evm_tx_receipt(tx(4)));
    }
}
#[test]
fn evm_receipt_native_canonical_fault_is_not_branch_denial_or_positive_fallback() {
    for fault in 0..3 {
        let n = Native::new();
        let genesis = n.node.params().genesis.hash;
        let broken = hash(0x12ec30 + fault);
        assert_ne!(broken, n.node.get_sink(), "no same-hash reachability shortcut");
        if fault == 0 {
            n.put(DatabaseStorePrefixes::Statuses, broken, &[0xff]);
        } else {
            // Storage-only status reaches the actual reachability reader; it
            // is never asserted to be an accepted block or claim.
            n.row(DatabaseStorePrefixes::Statuses, broken, &StatusUTXOValid);
            if fault == 2 {
                let prefix = DatabaseStorePrefixes::Reachability
                    .into_iter()
                    .chain([u8::from(DatabaseStorePrefixes::Separator)])
                    .collect::<Vec<_>>();
                n.db.put(DbKey::new(&prefix, broken), [0xff]).unwrap();
            }
        }
        n.receipt(genesis, tx(5), 12);
        n.index(tx(5), vec![(genesis, 0), (broken, 0)]);
        read_fault(n.node.get_evm_tx_receipt(tx(5)));
    }
}
#[tokio::test]
async fn evm_receipt_native_real_ancestor_sidebranch_and_legacy_absence_remain_distinct() {
    let mut n = Native::new();
    let genesis = n.node.params().genesis.hash;
    let siblings = [hash(0x12ec40), hash(0x12ec41)];
    for block in siblings {
        n.node.add_empty_utxo_valid_block_with_parents(block, vec![genesis]).await.unwrap();
    }
    let ancestor = n.node.get_sink();
    let side = siblings.into_iter().find(|h| *h != ancestor).unwrap();
    n.node.add_empty_utxo_valid_block_with_parents(hash(0x12ec42), vec![ancestor]).await.unwrap();
    assert_ne!(ancestor, n.node.get_sink());
    assert!(n.node.is_chain_block(ancestor).unwrap());
    assert!(!n.node.is_chain_block(side).unwrap());
    n.stop();
    n.row(
        DatabaseStorePrefixes::EvmReceipts,
        ancestor,
        &EvmBlockReceipts {
            tx_hashes: vec![tx(6), tx(7)],
            receipts: vec![
                EvmReceipt { logs: vec![EvmLog::default(); 2], ..Default::default() },
                EvmReceipt { succeeded: true, gas_used: 22_345, logs: vec![EvmLog::default()], ..Default::default() },
            ],
        },
    );
    n.row(DatabaseStorePrefixes::EvmHeader, ancestor, &EvmExecutionHeader { evm_number: 42, ..Default::default() });
    // A real side branch remains absent and is not read even if its receipts
    // are locally corrupt. An unknown accepting header preserves the old skip.
    n.put(DatabaseStorePrefixes::EvmReceipts, side, &[0xff]);
    n.index(tx(7), vec![(ancestor, 1), (side, 0), (hash(0x12ec99), 0)]);
    let result = n.node.get_evm_tx_receipt(tx(7)).unwrap().unwrap();
    assert_eq!((result.accepting_block, result.evm_number, result.receipt_index, result.log_index_offset), (ancestor, 42, 1, 2));
    assert_eq!(result.receipt.gas_used, 22_345);
    assert!(result.receipt.succeeded);
    n.index(tx(8), vec![(side, 0)]);
    assert!(n.node.get_evm_tx_receipt(tx(8)).unwrap().is_none());
    n.index(tx(9), vec![(ancestor, 99)]);
    assert!(n.node.get_evm_tx_receipt(tx(9)).unwrap().is_none());
    n.index(tx(10), vec![(ancestor, 1)]);
    assert!(n.node.get_evm_tx_receipt(tx(10)).unwrap().is_none(), "hash mismatch remains defensive absence");
    n.index(tx(11), vec![(genesis, 0)]);
    assert!(n.node.get_evm_tx_receipt(tx(11)).unwrap().is_none(), "true absent receipt rows stay absent");
}
