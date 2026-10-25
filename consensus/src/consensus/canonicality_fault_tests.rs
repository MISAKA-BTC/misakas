//! Production canonicality queries distinguish real branch decisions from local
//! status/reachability faults. Cold row fixtures confer no claim authority.
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::{
    blockstatus::BlockStatus::{StatusInvalid, StatusUTXOValid},
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    evm::EvmExecutionHeader,
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
    fn put(&self, prefix: DatabaseStorePrefixes, key: impl Clone + AsRef<[u8]>, raw: &[u8]) {
        self.db.put(DbKey::new(&Vec::<u8>::from(prefix), key), raw).unwrap();
    }
    fn row(&self, prefix: DatabaseStorePrefixes, key: impl Clone + AsRef<[u8]>, row: &impl Serialize) {
        self.put(prefix, key, &bincode::serialize(row).unwrap());
    }
    fn number(&self, number: u64, block: BlockHash) {
        let mut key = [0; 32];
        key[24..].copy_from_slice(&number.to_be_bytes());
        self.row(DatabaseStorePrefixes::EvmNumberIndex, key, &block);
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.node.shutdown(std::mem::take(&mut self.handles));
    }
}
fn hash(n: u64) -> BlockHash {
    BlockHash::from_u64_word(n)
}
fn read_error<T: std::fmt::Debug>(result: ConsensusResult<T>) {
    assert!(matches!(result, Err(ConsensusError::GeneralOwned(_))), "expected typed local read failure: {result:?}");
}

#[test]
fn native_canonicality_cold_status_fault_is_not_missing_or_invalid() {
    let n = Native::new();
    let genesis = n.node.params().genesis.hash;
    let broken = hash(0x12ca01);
    assert_ne!(broken, n.node.get_sink(), "do not query an equal-hash shortcut");
    n.put(DatabaseStorePrefixes::Statuses, broken, &[0xff]);
    n.number(100, broken);
    read_error(n.node.is_chain_block(broken));
    read_error(n.node.is_chain_ancestor_of(genesis, broken));
    read_error(n.node.get_evm_block_by_number(100));
    for (id, status) in [(0x12ca02, None), (0x12ca03, Some(StatusInvalid))] {
        let target = hash(id);
        if let Some(status) = status {
            n.row(DatabaseStorePrefixes::Statuses, target, &status);
        }
        n.number(id, target);
        assert!(matches!(n.node.is_chain_block(target), Err(ConsensusError::HeaderNotFound(h)) if h == target));
        assert!(matches!(n.node.get_evm_block_by_number(id), Err(ConsensusError::HeaderNotFound(h)) if h == target));
    }
}

#[test]
fn native_canonicality_missing_or_corrupt_reachability_is_not_branch_denial() {
    let n = Native::new();
    let genesis = n.node.params().genesis.hash;
    for (id, corrupt) in [(0x12ca10, false), (0x12ca11, true)] {
        let target = hash(id);
        assert_ne!(target, genesis);
        // This explicit storage-only status lets the query reach the real
        // reachability backend. It does not construct an accepted block.
        n.row(DatabaseStorePrefixes::Statuses, target, &StatusUTXOValid);
        if corrupt {
            n.put(DatabaseStorePrefixes::Reachability, target, &[0xff]);
        }
        n.number(id, target);
        read_error(n.node.is_chain_block(target));
        read_error(n.node.is_chain_ancestor_of(genesis, target));
        read_error(n.node.get_evm_block_by_number(id));
    }
}

#[tokio::test]
async fn native_canonicality_real_ancestor_and_sibling_keep_exact_evm_decisions() {
    let n = Native::new();
    let genesis = n.node.params().genesis.hash;
    for id in [0x12ca20, 0x12ca21] {
        n.node.add_empty_utxo_valid_block_with_parents(hash(id), vec![genesis]).await.unwrap();
    }
    let tip = n.node.get_sink();
    assert_ne!(tip, genesis);
    assert!(n.node.is_chain_ancestor_of(genesis, tip).unwrap(), "real accepted distinct ancestor remains true");
    let side = [hash(0x12ca20), hash(0x12ca21)]
        .into_iter()
        .find(|h| !n.node.is_chain_block(*h).unwrap())
        .expect("real accepted sibling outside selected-parent chain");
    assert_ne!(side, tip);
    n.number(200, side);
    assert!(n.node.get_evm_block_by_number(200).unwrap().is_none(), "real noncanonical target stays absent");
    n.number(201, tip);
    n.row(DatabaseStorePrefixes::EvmHeader, tip, &EvmExecutionHeader { evm_number: 201, ..Default::default() });
    assert_eq!(n.node.get_evm_block_by_number(201).unwrap().unwrap().l1_hash, tip);
}
