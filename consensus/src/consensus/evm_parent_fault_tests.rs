//! Exact immutable selected-parent identity. Native branches are real; EVM
//! headers/codec failures are explicit storage fixtures, not executed EVM work.
use super::*;
use crate::{consensus::test_consensus::TestConsensus, model::stores::ghostdag::CompactGhostdagData};
use kaspa_consensus_core::{
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    evm::{EVM_GENESIS_STATE_ROOT, EvmExecutionHeader},
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
    fn put(&self, prefix: DatabaseStorePrefixes, key: BlockHash, bytes: &[u8]) {
        let mut namespace = Vec::<u8>::from(prefix);
        // The real Ghostdag store nests rows under the native block level.
        if matches!(prefix, DatabaseStorePrefixes::GhostdagCompact) {
            namespace.push(0);
        }
        self.db.put(DbKey::new(&namespace, key), bytes).unwrap();
    }
    fn row(&self, prefix: DatabaseStorePrefixes, key: BlockHash, row: &impl Serialize) {
        self.put(prefix, key, &bincode::serialize(row).unwrap());
    }
    fn header(&self, key: BlockHash, number: u64, parent: kaspa_hashes::EvmH256, state: kaspa_hashes::EvmH256) {
        self.row(
            DatabaseStorePrefixes::EvmHeader,
            key,
            &EvmExecutionHeader { evm_number: number, parent_state_root: parent, state_root: state, ..Default::default() },
        );
    }
    fn link(&self, child: BlockHash, parent: BlockHash) {
        self.row(
            DatabaseStorePrefixes::GhostdagCompact,
            child,
            &CompactGhostdagData { blue_score: 0, blue_work: Default::default(), selected_parent: parent },
        );
    }
    fn daa(&self, parent: BlockHash, daa: u64) {
        self.row(
            DatabaseStorePrefixes::HeadersCompact,
            parent,
            &CompactHeaderData { daa_score: daa, timestamp: 0, bits: 0, blue_score: 0 },
        );
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
fn root(id: u8) -> kaspa_hashes::EvmH256 {
    kaspa_hashes::EvmH256::from_bytes([id; 32])
}
#[tokio::test]
async fn evm_parent_native_retained_forks_keep_selected_parent_under_number_index_rebase() {
    let mut n = Native::new();
    let genesis = n.node.params().genesis.hash;
    for (p, c) in [(block(11), block(12)), (block(21), block(22))] {
        n.node.add_empty_utxo_valid_block_with_parents(p, vec![genesis]).await.unwrap();
        n.node.add_empty_utxo_valid_block_with_parents(c, vec![p]).await.unwrap();
    }
    n.stop();
    assert!(
        !n.node.is_chain_block(block(12)).unwrap() || !n.node.is_chain_block(block(22)).unwrap(),
        "real fork outside selected chain"
    );
    for (p, c, id) in [(block(11), block(12), 1), (block(21), block(22), 2)] {
        n.header(p, 1, EVM_GENESIS_STATE_ROOT, root(id));
        n.header(c, 2, root(id), root(id + 10));
        // Receipts and raw transactions are irrelevant to parent identity.
        n.put(DatabaseStorePrefixes::EvmReceipts, c, &[0xff]);
    }
    let mut key = [0u8; 32];
    key[31] = 1;
    for canonical in [block(11), block(21)] {
        n.db.put(DbKey::new(&Vec::<u8>::from(DatabaseStorePrefixes::EvmNumberIndex), key), bincode::serialize(&canonical).unwrap())
            .unwrap();
        assert_eq!(n.node.get_evm_parent_l1_hash(block(12)).unwrap(), Some(block(11)));
        assert_eq!(n.node.get_evm_parent_l1_hash(block(22)).unwrap(), Some(block(21)));
    }
    n.db.put(DbKey::new(&Vec::<u8>::from(DatabaseStorePrefixes::EvmNumberIndex), key), [0xff]).unwrap();
    assert_eq!(n.node.get_evm_parent_l1_hash(block(12)).unwrap(), Some(block(11)));
}
#[test]
fn evm_parent_native_cold_codec_faults_never_become_zero_or_canonical_parent() {
    use crate::model::stores::evm::EvmHeaderStoreReader;
    use kaspa_database::prelude::StoreError;

    for which in 0..4 {
        let n = Native::new();
        let c = block(40);
        let p = block(41);
        n.header(c, 2, root(1), root(2));
        if which == 0 {
            n.put(DatabaseStorePrefixes::EvmHeader, c, &[0xff]);
            assert!(matches!(n.node.storage.evm_header_store.get(c), Err(StoreError::DeserializationError(_))));
        } else if which == 1 {
            n.put(DatabaseStorePrefixes::GhostdagCompact, c, &[0xff]);
            assert!(matches!(n.node.ghostdag_store.get_selected_parent(c), Err(StoreError::DeserializationError(_))));
        } else {
            n.link(c, p);
            assert_eq!(n.node.ghostdag_store.get_selected_parent(c).unwrap(), p);
            if which == 2 {
                n.put(DatabaseStorePrefixes::EvmHeader, p, &[0xff]);
                assert!(matches!(n.node.storage.evm_header_store.get(p), Err(StoreError::DeserializationError(_))));
            } else {
                n.put(DatabaseStorePrefixes::HeadersCompact, p, &[0xff]);
                assert!(matches!(n.node.headers_store.get_daa_score(p), Err(StoreError::DeserializationError(_))));
            }
        }
        assert!(matches!(n.node.get_evm_parent_l1_hash(c), Err(ConsensusError::GeneralOwned(_))), "cold fault {which}");
    }
}
#[test]
fn evm_parent_native_only_true_execution_boundary_has_no_parent() {
    let n = Native::new();
    let genesis = n.node.params().genesis.hash;
    assert!(matches!(n.node.get_evm_parent_l1_hash(block(50)), Err(ConsensusError::MissingData(_))));
    n.header(block(51), 0, root(0), root(1));
    assert_eq!(n.node.get_evm_parent_l1_hash(block(51)).unwrap(), None);
    n.header(block(52), 1, EVM_GENESIS_STATE_ROOT, root(1));
    n.link(block(52), genesis);
    assert_eq!(n.node.get_evm_parent_l1_hash(block(52)).unwrap(), None);
    let activation = n.node.params().evm_activation_daa_score;
    assert!(activation > 0, "mainnet preactivation storage fixture");
    n.header(block(53), 1, EVM_GENESIS_STATE_ROOT, root(1));
    n.link(block(53), block(54));
    n.daa(block(54), activation - 1);
    assert_eq!(n.node.get_evm_parent_l1_hash(block(53)).unwrap(), None);
    n.header(block(55), 1, EVM_GENESIS_STATE_ROOT, root(1));
    n.link(block(55), block(56));
    n.daa(block(56), activation);
    assert!(matches!(n.node.get_evm_parent_l1_hash(block(55)),Err(ConsensusError::MissingData(p)) if p==block(56)));
}
#[test]
fn evm_parent_native_number_root_overflow_and_absent_history_are_refused() {
    for which in 0..4 {
        let n = Native::new();
        let c = block(60);
        let p = block(61);
        n.link(c, p);
        n.header(c, 2, root(1), root(2));
        match which {
            0 => n.header(p, 5, root(0), root(1)),
            1 => n.header(p, 1, root(0), root(99)),
            2 => n.header(p, u64::MAX, root(0), root(1)),
            _ => n.daa(p, 0),
        }
        assert_eq!(n.node.ghostdag_store.get_selected_parent(c).unwrap(), p);
        let actual = n.node.get_evm_parent_l1_hash(c);
        if which < 3 {
            assert!(
                matches!(&actual, Err(ConsensusError::General(reason))
                if *reason == "immutable EVM parent number/state-root mismatch"),
                "invalid parent {which}: {actual:?}"
            );
        } else {
            assert!(matches!(&actual, Err(ConsensusError::MissingData(missing)) if *missing == p), "{actual:?}");
        }
    }
}
