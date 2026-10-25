//! Cold real-store query faults; not EVM execution/acceptance or campaign evidence.
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::{
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    evm::{EVM_GENESIS_STATE_ROOT, EvmExecutionHeader, EvmLatestStatePtr, EvmStateCheckpointV1, EvmStateSnapshot},
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
    fn put(&self, prefix: DatabaseStorePrefixes, block: BlockHash, bytes: &[u8]) {
        self.db.put(DbKey::new(&Vec::<u8>::from(prefix), block), bytes).unwrap();
    }
    fn row(&self, prefix: DatabaseStorePrefixes, block: BlockHash, value: &impl Serialize) {
        self.put(prefix, block, &bincode::serialize(value).unwrap());
    }
    fn pointer(&self, bytes: &[u8]) {
        self.db.put(DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::EvmLatestStatePtr)), bytes).unwrap();
    }
    fn reconstruction(&self, block: BlockHash) -> EvmStateSnapshot {
        let snapshot = EvmStateSnapshot::default();
        self.row(
            DatabaseStorePrefixes::EvmHeader,
            block,
            &EvmExecutionHeader { evm_number: 7, state_root: EVM_GENESIS_STATE_ROOT, ..Default::default() },
        );
        self.row(
            DatabaseStorePrefixes::EvmStateCheckpoint,
            block,
            &EvmStateCheckpointV1::build(block, 7, EVM_GENESIS_STATE_ROOT, &snapshot),
        );
        assert_eq!(
            self.node.reconstruct_evm_state_at(block).unwrap(),
            Some(snapshot.clone()),
            "the alternative is an actual successful root-verified reconstruction"
        );
        snapshot
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.node.shutdown(std::mem::take(&mut self.handles));
    }
}
fn hash(id: u8) -> BlockHash {
    BlockHash::from_bytes([id; 64])
}
fn read_fault<T: std::fmt::Debug>(result: ConsensusResult<T>, context: &str) {
    match result {
        Err(ConsensusError::GeneralOwned(message)) => assert!(message.contains(context), "{message}"),
        other => panic!("expected typed {context} error, got {other:?}"),
    }
}

#[test]
fn native_evm_state_cold_hot_corruption_cannot_be_hidden_by_valid_reconstruction() {
    let n = Native::new();
    let target = hash(0xc1);
    let expected = n.reconstruction(target);
    // Prefix 206 has not been queried/cached before this real malformed write.
    n.put(DatabaseStorePrefixes::EvmStateDiff, target, &[0xff]);
    read_fault(n.node.get_evm_state_snapshot_of(target), "hot snapshot read");
    assert_eq!(n.node.reconstruct_evm_state_at(target).unwrap(), Some(expected));
}

#[test]
fn native_evm_state_cold_flat_pointer_fault_is_not_missing_or_stale() {
    let n = Native::new();
    let target = hash(0xc2);
    let expected = n.reconstruction(target);
    n.pointer(&[0xff]);
    read_fault(n.node.get_evm_state_snapshot_of(target), "flat state pointer read");
    assert_eq!(n.node.reconstruct_evm_state_at(target).unwrap(), Some(expected.clone()));
    // A valid exact pointer keeps the existing materialized-flat path.
    n.pointer(&bincode::serialize(&EvmLatestStatePtr { canonical_head: target, state_root: EVM_GENESIS_STATE_ROOT }).unwrap());
    assert_eq!(n.node.get_evm_state_snapshot_of(target).unwrap(), Some(expected));
    // A valid pointer at a different block keeps the existing reconstruction path.
    let other = hash(0xc7);
    let other_expected = n.reconstruction(other);
    assert_eq!(n.node.get_evm_state_snapshot_of(other).unwrap(), Some(other_expected));
    // A separate cold instance prevents a previously cached valid pointer
    // from disguising this control: actual corrupt 231 is not read at all
    // when a valid authoritative 206 row is already present.
    let hot = Native::new();
    hot.row(DatabaseStorePrefixes::EvmStateDiff, hash(0xc3), &EvmStateSnapshot::default());
    hot.pointer(&[0xff]);
    assert_eq!(hot.node.get_evm_state_snapshot_of(hash(0xc3)).unwrap(), Some(EvmStateSnapshot::default()));
}

#[test]
fn native_evm_state_target_header_fault_absence_and_positive_remain_distinct() {
    let n = Native::new();
    assert_eq!(n.node.reconstruct_evm_state_at(hash(0xc4)).unwrap(), None);
    assert_eq!(n.node.get_evm_state_snapshot_of(hash(0xc4)).unwrap(), None);
    n.put(DatabaseStorePrefixes::EvmHeader, hash(0xc5), &[0xff]);
    read_fault(n.node.reconstruct_evm_state_at(hash(0xc5)), "target header read");
    read_fault(n.node.get_evm_state_snapshot_of(hash(0xc5)), "target header read");
    let expected = n.reconstruction(hash(0xc6));
    assert_eq!(n.node.get_evm_state_snapshot_of(hash(0xc6)).unwrap(), Some(expected));
}
