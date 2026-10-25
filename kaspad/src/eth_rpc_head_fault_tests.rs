//! Real node renderer/notification helpers with fault-injected ConsensusApi reads.
use super::*;
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::EvmBlockResponse,
};
use std::{
    cell::Cell,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

fn hash(id: u64) -> BlockHash {
    BlockHash::from_bytes([id as u8; 64])
}
fn block(id: u64, number: u64) -> EvmBlockResponse {
    EvmBlockResponse {
        header: kaspa_consensus_core::evm::EvmExecutionHeader { evm_number: number, ..Default::default() },
        l1_hash: hash(id),
        tx_hashes: vec![],
        encoded_size: 0,
    }
}
fn fault<T>() -> ConsensusResult<T> {
    Err(ConsensusError::GeneralOwned("cold block row unavailable".into()))
}
struct Reader {
    parent: ConsensusResult<Option<BlockHash>>,
    parent_reads: Mutex<Vec<BlockHash>>,
    child_fault: AtomicU64,
    child_reads: Mutex<Vec<BlockHash>>,
}
impl Reader {
    fn new() -> Self {
        Self {
            parent: Ok(Some(hash(8))),
            parent_reads: Mutex::new(vec![]),
            child_fault: AtomicU64::new(0),
            child_reads: Mutex::new(vec![]),
        }
    }
}
impl ConsensusApi for Reader {
    fn get_evm_block_by_number(&self, _: u64) -> ConsensusResult<Option<EvmBlockResponse>> {
        panic!("immutable parent rendering must not substitute a canonical number lookup")
    }
    fn get_evm_parent_l1_hash(&self, block: BlockHash) -> ConsensusResult<Option<BlockHash>> {
        self.parent_reads.lock().unwrap().push(block);
        self.parent.clone()
    }
    fn get_evm_block_by_l1_hash(&self, target: BlockHash) -> ConsensusResult<Option<EvmBlockResponse>> {
        self.child_reads.lock().unwrap().push(target);
        let id = [1, 2, 3].into_iter().find(|id| hash(*id) == target).unwrap();
        if self.child_fault.load(Ordering::SeqCst) == id {
            return fault();
        }
        Ok((id != 3).then(|| block(id, 1)))
    }
}
#[test]
fn head_rpc_primary_fault_and_unknown_do_not_query_parent_or_render() {
    struct Unavailable;
    impl ConsensusApi for Unavailable {}
    assert!(
        checked_block_with_parent_v3(&Unavailable, Ok(Some(block(1, 1)))).is_err(),
        "an unimplemented immutable-parent API is unavailable, never a fabricated zero parent"
    );
    let reader = Reader::new();
    assert!(checked_block_with_parent_v3(&reader, fault()).is_err());
    assert!(checked_block_with_parent_v3(&reader, Ok(None)).unwrap().is_none());
    assert!(reader.parent_reads.lock().unwrap().is_empty());
    let genesis = checked_block_with_parent_v3(&reader, Ok(Some(block(1, 0)))).unwrap().unwrap();
    assert_eq!(genesis.parent_hash, [0; 32]);
    assert!(reader.parent_reads.lock().unwrap().is_empty(), "genesis needs no parent lookup");
}
#[test]
fn head_rpc_parent_fault_is_not_genesis_but_true_absence_stays_zero() {
    let mut reader = Reader::new();
    reader.parent = fault();
    assert!(checked_block_with_parent_v3(&reader, Ok(Some(block(1, 1)))).is_err());
    assert_eq!(*reader.parent_reads.lock().unwrap(), [hash(1)]);
    reader.parent = Ok(None);
    assert_eq!(checked_block_with_parent_v3(&reader, Ok(Some(block(1, 1)))).unwrap().unwrap().parent_hash, [0; 32]);
    reader.parent = Ok(Some(hash(8)));
    let actual = checked_block_with_parent_v3(&reader, Ok(Some(block(1, 1)))).unwrap().unwrap();
    assert_eq!(actual.parent_hash.as_slice(), &hash(8).as_bytes()[..32]);
    assert_ne!(actual.parent_hash, [0; 32]);
    assert_eq!(actual.number, 1);
    assert_eq!(actual.hash.as_slice(), &hash(1).as_bytes()[..32]);
}
#[test]
fn head_notification_primary_or_parent_fault_cannot_return_partial_heads() {
    let mut reader = Reader::new();
    let added = [hash(1), hash(2), hash(3)];
    reader.child_fault.store(2, Ordering::SeqCst);
    assert!(collect_heads_checked_v3(&reader, &added).is_err());
    assert_eq!(*reader.child_reads.lock().unwrap(), added[..2]);
    reader.child_fault.store(0, Ordering::SeqCst);
    reader.parent = fault();
    assert!(collect_heads_checked_v3(&reader, &added).is_err());
    reader.parent = Ok(None);
    let heads = collect_heads_checked_v3(&reader, &added).unwrap();
    assert_eq!(heads.len(), 2, "genuine non-EVM block remains skipped");
    assert!(heads.iter().all(|h| h.parent_hash == [0; 32]));
}
#[tokio::test]
async fn head_notification_retry_publishes_once_before_separate_log_stage() {
    let reader = Reader::new();
    reader.child_fault.store(2, Ordering::SeqCst);
    let attempts = Cell::new(0);
    let shutdown = SingleTrigger::default();
    let added = [hash(1), hash(2)];
    let heads = retry_reorg_logs_v2(
        || {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 2 {
                reader.child_fault.store(0, Ordering::SeqCst);
            }
            std::future::ready(collect_heads_checked_v3(&reader, &added))
        },
        || true,
        shutdown.listener.clone(),
        Duration::from_millis(1),
    )
    .await
    .unwrap();
    let (heads_tx, mut heads_rx) = broadcast::channel(8);
    for head in heads {
        heads_tx.send(head).unwrap();
    }
    assert_eq!(heads_rx.try_recv().unwrap().hash.as_slice(), &added[0].as_bytes()[..32]);
    assert_eq!(heads_rx.try_recv().unwrap().hash.as_slice(), &added[1].as_bytes()[..32]);
    assert!(heads_rx.try_recv().is_err());
    let logs_attempts = Cell::new(0);
    let logs = retry_reorg_logs_v2(
        || {
            logs_attempts.set(logs_attempts.get() + 1);
            std::future::ready(if logs_attempts.get() == 1 { Err("receipt read") } else { Ok(vec![1u8]) })
        },
        || true,
        shutdown.listener,
        Duration::from_millis(1),
    )
    .await
    .unwrap();
    assert_eq!(logs, [1]);
    assert_eq!(attempts.get(), 2, "log retries do not repeat the completed head stage");
    assert!(heads_rx.try_recv().is_err());
}
