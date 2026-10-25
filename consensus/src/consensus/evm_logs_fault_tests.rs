//! Actual native log readers and cold stores. L1 ancestor/side branches are
//! accepted by the real pipeline; EVM rows are storage-only fixtures, not EVM
//! execution, bridge settlement, or maturity/certification evidence.
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::{
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    evm::*,
};
use kaspa_database::{
    create_temp_db,
    prelude::{ConnBuilder, DbKey, DbSetAccess, DirectDbWriter},
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
    fn stop(&mut self) {
        self.node.shutdown(std::mem::take(&mut self.handles));
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.stop();
    }
}

fn block(id: u8) -> BlockHash {
    BlockHash::from_bytes([id; 64])
}
fn rpc(block: BlockHash) -> kaspa_hashes::EvmH256 {
    kaspa_hashes::EvmH256::from_bytes(block.as_bytes()[..32].try_into().unwrap())
}
fn address() -> EvmAddress {
    EvmAddress::from_bytes([7; 20])
}
fn number(n: u64) -> kaspa_hashes::EvmH256 {
    let mut k = [0; 32];
    k[24..].copy_from_slice(&n.to_be_bytes());
    kaspa_hashes::EvmH256::from_bytes(k)
}
fn read_fault<T: std::fmt::Debug>(actual: ConsensusResult<T>) {
    assert!(matches!(actual, Err(ConsensusError::GeneralOwned(_))), "{actual:?}");
}
impl Native {
    fn logs(&self, b: BlockHash, n: u64, data: u8) {
        self.row(DatabaseStorePrefixes::EvmHeader, b, &EvmExecutionHeader { evm_number: n, ..Default::default() });
        self.row(
            DatabaseStorePrefixes::EvmReceipts,
            b,
            &EvmBlockReceipts {
                tx_hashes: vec![rpc(b)],
                receipts: vec![EvmReceipt {
                    logs: vec![EvmLog { address: address(), topics: vec![rpc(b)], data: vec![data] }],
                    ..Default::default()
                }],
            },
        );
        self.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(b), &b);
    }
    fn floor(&self) {
        self.row(DatabaseStorePrefixes::EvmLogIndexMeta, number(0), &0u64);
    }
    fn posting(&self, b: BlockHash, n: u64) {
        let mut batch = rocksdb::WriteBatch::default();
        self.node
            .storage
            .evm_log_index_store
            .write_posting_batch(
                &mut batch,
                LogPostingKind::Address,
                &address().as_bytes(),
                &LogPostingLoc { evm_number: n, l1_hash: b, tx_index: 0, in_receipt_log_index: 0 },
            )
            .unwrap();
        self.db.write(batch).unwrap();
    }
}
#[tokio::test]
async fn evm_logs_native_exact_side_hash_never_substitutes_same_number_canonical_logs() {
    let mut n = Native::new();
    let genesis = n.node.params().genesis.hash;
    let siblings = [block(41), block(42)];
    for b in siblings {
        n.node.add_empty_utxo_valid_block_with_parents(b, vec![genesis]).await.unwrap();
    }
    let ancestor = n.node.get_sink();
    let side = siblings.into_iter().find(|b| *b != ancestor).unwrap();
    n.node.add_empty_utxo_valid_block_with_parents(block(43), vec![ancestor]).await.unwrap();
    assert!(n.node.is_chain_block(ancestor).unwrap());
    assert!(!n.node.is_chain_block(side).unwrap());
    assert_ne!(ancestor, n.node.get_sink());
    n.stop();
    n.logs(ancestor, 42, 1);
    n.logs(side, 42, 2);
    n.row(DatabaseStorePrefixes::EvmNumberIndex, number(42), &ancestor);
    let exact = n.node.get_evm_block_logs_by_rpc_hash(rpc(side)).unwrap().unwrap();
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0].block_l1_hash, side);
    assert_eq!(exact[0].data, [2]);
    let scan = n.node.get_evm_logs(42, 42, vec![address()], vec![]).unwrap();
    assert_eq!(scan.len(), 1);
    assert_eq!(scan[0].block_l1_hash, ancestor);
    assert_eq!(scan[0].data, [1]);
    n.floor();
    n.posting(ancestor, 42);
    n.posting(side, 42);
    assert_eq!(n.node.get_evm_logs(42, 42, vec![address(), address()], vec![]).unwrap(), scan);
    // A valid-shaped stale posting must not bypass the requested address filter.
    let wrong = EvmAddress::from_bytes([8; 20]);
    let mut batch = rocksdb::WriteBatch::default();
    n.node
        .storage
        .evm_log_index_store
        .write_posting_batch(
            &mut batch,
            LogPostingKind::Address,
            &wrong.as_bytes(),
            &LogPostingLoc { evm_number: 42, l1_hash: ancestor, tx_index: 0, in_receipt_log_index: 0 },
        )
        .unwrap();
    n.db.write(batch).unwrap();
    assert!(n.node.get_evm_logs(42, 42, vec![wrong], vec![]).unwrap().is_empty());
}
#[test]
fn evm_logs_native_cold_exact_hash_errors_are_not_missing_or_alternate_logs() {
    for fault in 0..4 {
        let n = Native::new();
        let b = block(50 + fault);
        let alternate = block(60 + fault);
        n.logs(alternate, 9, 3);
        n.row(DatabaseStorePrefixes::EvmNumberIndex, number(9), &alternate);
        if fault == 0 {
            n.put(DatabaseStorePrefixes::EvmBlockHashMap, rpc(b), &[0xff]);
        } else if fault == 1 {
            n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(b), &b);
            n.put(DatabaseStorePrefixes::EvmHeader, b, &[0xff]);
        } else if fault == 2 {
            n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(b), &b);
            n.row(DatabaseStorePrefixes::EvmHeader, b, &EvmExecutionHeader { evm_number: 9, ..Default::default() });
            n.put(DatabaseStorePrefixes::EvmReceipts, b, &[0xff]);
        } else {
            n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(b), &alternate);
        }
        read_fault(n.node.get_evm_block_logs_by_rpc_hash(rpc(b)));
    }
    let n = Native::new();
    assert!(n.node.get_evm_block_logs_by_rpc_hash(rpc(block(70))).unwrap().is_none());
    n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(block(71)), &block(71));
    assert!(n.node.get_evm_block_logs_by_rpc_hash(rpc(block(71))).unwrap().is_none());
    n.row(DatabaseStorePrefixes::EvmBlockHashMap, rpc(block(72)), &block(72));
    n.row(DatabaseStorePrefixes::EvmHeader, block(72), &EvmExecutionHeader::default());
    assert_eq!(n.node.get_evm_block_logs_by_rpc_hash(rpc(block(72))).unwrap(), Some(vec![]));
}
#[test]
fn evm_logs_native_cold_range_reads_propagate_in_scan_and_index_paths() {
    for indexed in [false, true] {
        for broken_header in [false, true] {
            let n = Native::new();
            let b = n.node.params().genesis.hash;
            n.row(DatabaseStorePrefixes::EvmNumberIndex, number(7), &b);
            if broken_header {
                n.put(DatabaseStorePrefixes::EvmHeader, b, &[0xff]);
            } else {
                n.row(DatabaseStorePrefixes::EvmHeader, b, &EvmExecutionHeader { evm_number: 7, ..Default::default() });
                n.put(DatabaseStorePrefixes::EvmReceipts, b, &[0xff]);
            }
            if indexed {
                n.floor();
                n.posting(b, 7);
            }
            read_fault(n.node.get_evm_logs(7, 7, if indexed { vec![address()] } else { vec![] }, vec![]));
        }
    }
    let n = Native::new();
    n.put(DatabaseStorePrefixes::EvmNumberIndex, number(7), &[0xff]);
    read_fault(n.node.get_evm_logs(7, 7, vec![], vec![]));
}
#[test]
fn evm_logs_native_index_completeness_and_malformed_member_faults_cannot_succeed_via_scan() {
    for fault in 0..3 {
        let n = Native::new();
        let b = n.node.params().genesis.hash;
        n.logs(b, 7, 9);
        n.row(DatabaseStorePrefixes::EvmNumberIndex, number(7), &b);
        if fault == 1 {
            n.floor();
            let access = DbSetAccess::<Vec<u8>, Vec<u8>>::new(n.db.clone(), DatabaseStorePrefixes::EvmLogs.into());
            access
                .write(DirectDbWriter::new(&n.db), log_posting_bucket(LogPostingKind::Address, &address().as_bytes()), vec![0xff])
                .unwrap();
        } else if fault == 2 {
            n.floor();
            // Corrupt the set member's serialized Vec envelope, not just its
            // validly serialized but malformed posting payload above.
            let bucket = log_posting_bucket(LogPostingKind::Address, &address().as_bytes());
            n.db.put(DbKey::new_with_bucket(&Vec::<u8>::from(DatabaseStorePrefixes::EvmLogs), bucket.as_slice(), [0xff]), []).unwrap();
        } else {
            n.put(DatabaseStorePrefixes::EvmLogIndexMeta, number(0), &[0xff]);
        }
        read_fault(n.node.get_evm_logs(7, 7, vec![address()], vec![]));
        assert_eq!(
            n.node.get_evm_logs(7, 7, vec![], vec![]).unwrap().len(),
            1,
            "the independent canonical scan is a real positive alternative, never a fault fallback"
        );
    }
}
#[test]
fn evm_logs_native_range_canonical_read_fault_is_not_false_or_empty() {
    for indexed in [false, true] {
        for missing_reachability in [false, true] {
            let n = Native::new();
            let b = block(90);
            assert_ne!(b, n.node.get_sink());
            if missing_reachability {
                // Storage-only status exercises a failed actual reachability read;
                // this synthetic hash is never called an accepted block.
                n.row(DatabaseStorePrefixes::Statuses, b, &kaspa_consensus_core::blockstatus::BlockStatus::StatusUTXOValid);
            } else {
                n.put(DatabaseStorePrefixes::Statuses, b, &[0xff]);
            }
            n.row(DatabaseStorePrefixes::EvmNumberIndex, number(7), &b);
            if indexed {
                n.floor();
                n.posting(b, 7);
            }
            read_fault(n.node.get_evm_logs(7, 7, if indexed { vec![address()] } else { vec![] }, vec![]));
        }
    }
    let n = Native::new();
    n.row(DatabaseStorePrefixes::EvmNumberIndex, number(7), &block(91));
    assert!(n.node.get_evm_logs(7, 7, vec![], vec![]).unwrap().is_empty(), "true unknown retains skip contract");
}
