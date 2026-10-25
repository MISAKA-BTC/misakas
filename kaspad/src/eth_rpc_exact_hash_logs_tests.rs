//! Actual node hash-log continuation/filter boundary, not executed bridge logs.
use super::*;
use kaspa_consensus_core::{
    api::ConsensusApi,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::*,
};
use std::sync::Mutex;
struct Reader {
    result: ConsensusResult<Option<Vec<EvmLogEntry>>>,
    calls: Mutex<Vec<EvmH256>>,
}
impl ConsensusApi for Reader {
    fn get_evm_block_logs_by_rpc_hash(&self, hash: EvmH256) -> ConsensusResult<Option<Vec<EvmLogEntry>>> {
        self.calls.lock().unwrap().push(hash);
        self.result.clone()
    }
    fn get_evm_logs(&self, _: u64, _: u64, _: Vec<EvmAddress>, _: Vec<Vec<EvmH256>>) -> ConsensusResult<Vec<EvmLogEntry>> {
        panic!("hash query must not resolve a numeric range")
    }
}
fn log(hash: u8, address: u8, topics: Vec<u8>) -> EvmLogEntry {
    EvmLogEntry {
        address: EvmAddress::from_bytes([address; 20]),
        topics: topics.into_iter().map(|t| EvmH256::from_bytes([t; 32])).collect(),
        data: vec![9],
        block_number: 42,
        block_l1_hash: kaspa_consensus_core::BlockHash::from_bytes([hash; 64]),
        tx_hash: EvmH256::from_bytes([4; 32]),
        tx_index: 2,
        log_index: 5,
    }
}
fn reader(result: ConsensusResult<Option<Vec<EvmLogEntry>>>) -> Reader {
    Reader { result, calls: Mutex::new(vec![]) }
}
#[test]
fn exact_hash_logs_node_fault_missing_and_identity_failures_do_not_become_empty() {
    struct Unavailable;
    impl ConsensusApi for Unavailable {}
    assert!(exact_hash_logs_v2(&Unavailable, [3; 32], vec![], vec![]).is_err(), "unimplemented exact API is unavailable, not unknown");
    for error in
        [ConsensusError::GeneralOwned("cold immutable log read".into()), ConsensusError::General("exact hash API unavailable")]
    {
        let r = reader(Err(error));
        assert!(exact_hash_logs_v2(&r, [3; 32], vec![], vec![]).is_err());
        assert_eq!(*r.calls.lock().unwrap(), [EvmH256::from_bytes([3; 32])]);
    }
    let r = reader(Ok(None));
    assert!(exact_hash_logs_v2(&r, [3; 32], vec![], vec![]).unwrap().is_none());
    let r = reader(Ok(Some(vec![])));
    assert!(exact_hash_logs_v2(&r, [3; 32], vec![], vec![]).unwrap().unwrap().is_empty());
    // Identity is checked even if filters would hide the wrong sibling's logs.
    let r = reader(Ok(Some(vec![log(4, 8, vec![])])));
    assert!(exact_hash_logs_v2(&r, [3; 32], vec![[7; 20]], vec![]).is_err());
}
#[test]
fn exact_hash_logs_node_exact_filters_preserve_block_and_log_coordinates() {
    let r = reader(Ok(Some(vec![log(3, 7, vec![1, 2]), log(3, 8, vec![1, 2]), log(3, 7, vec![1, 3]), log(3, 7, vec![1])])));
    let actual = exact_hash_logs_v2(&r, [3; 32], vec![[7; 20], [7; 20]], vec![vec![], vec![[2; 32], [4; 32]]]).unwrap().unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].block_hash, [3; 32]);
    assert_eq!((actual[0].block_number, actual[0].tx_index, actual[0].log_index), (42, 2, 5));
    assert_eq!(actual[0].data, [9]);
    assert_eq!(*r.calls.lock().unwrap(), [EvmH256::from_bytes([3; 32])]);
}
#[test]
fn exact_hash_logs_node_result_cap_errors_without_truncation_after_filters() {
    let r = reader(Ok(Some(vec![log(3, 7, vec![]); 10_001])));
    assert!(exact_hash_logs_v2(&r, [3; 32], vec![], vec![]).is_err());
    assert!(exact_hash_logs_v2(&r, [3; 32], vec![[8; 20]], vec![]).unwrap().unwrap().is_empty());
    let r = reader(Ok(Some(vec![log(3, 7, vec![]); 10_000])));
    assert_eq!(exact_hash_logs_v2(&r, [3; 32], vec![], vec![]).unwrap().unwrap().len(), 10_000);
}
