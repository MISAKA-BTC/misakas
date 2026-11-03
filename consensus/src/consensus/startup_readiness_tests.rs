//! The existing participation preflight must reject a recent persisted tip until
//! the actual processing workers exist; starting them does not bypass IBD flags.
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::{ConfigBuilder, params::MAINNET_PARAMS};

#[test]
fn startup_readiness_holds_until_workers_start_and_preserves_import_hold() {
    let node = TestConsensus::new(&ConfigBuilder::new(MAINNET_PARAMS).build());
    assert!(node.is_consensus_in_transitional_ibd_state(), "cached genesis/tip is not a running pipeline");
    let handles = node.init();
    assert!(!node.is_consensus_in_transitional_ibd_state(), "actual worker startup releases only the startup hold");
    let consensus = node.consensus_clone();
    let mut batch = rocksdb::WriteBatch::default();
    consensus.storage.pruning_meta_stores.write().set_pruning_utxoset_stable_flag(&mut batch, false).unwrap();
    consensus.db.write(batch).unwrap();
    assert!(node.is_consensus_in_transitional_ibd_state(), "running workers do not override an incomplete import");
    node.mark_pruning_utxoset_stable_for_tests();
    assert!(!node.is_consensus_in_transitional_ibd_state());
    node.shutdown(handles);
    assert!(node.is_consensus_in_transitional_ibd_state(), "stopped processors cannot authorize local work");
}
