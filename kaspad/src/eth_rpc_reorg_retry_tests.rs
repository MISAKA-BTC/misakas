//! Notification consumer controls, not a bridge execution/campaign fixture.
use super::*;
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::EvmLogEntry,
};
use std::{cell::Cell, sync::Mutex, time::Duration};

struct NativeReader {
    fail: std::sync::atomic::AtomicU64,
    calls: Mutex<Vec<u64>>,
}
impl ConsensusApi for NativeReader {
    fn get_evm_block_logs(&self, hash: BlockHash) -> ConsensusResult<Vec<EvmLogEntry>> {
        let n = [1, 2, 3, 4].into_iter().find(|n| BlockHash::from_u64_word(*n) == hash).unwrap();
        self.calls.lock().unwrap().push(n);
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) == n {
            return Err(ConsensusError::GeneralOwned("immutable receipt store unavailable".into()));
        }
        Ok(vec![EvmLogEntry {
            address: Default::default(),
            topics: vec![],
            data: vec![n as u8],
            block_number: n,
            block_l1_hash: hash,
            tx_hash: Default::default(),
            tx_index: 0,
            log_index: 0,
        }])
    }
}
fn read(reader: &NativeReader) -> ConsensusResult<Vec<(EvmLogEntry, bool)>> {
    reorg_ordered_checked_v2(
        &[BlockHash::from_u64_word(2), BlockHash::from_u64_word(1)],
        &[BlockHash::from_u64_word(3), BlockHash::from_u64_word(4)],
        |h| reader.get_evm_block_logs(h),
    )
}
#[test]
fn reorg_retry_native_fault_never_returns_a_partial_notification() {
    for (failed, reads) in [(1, vec![1]), (2, vec![1, 2]), (3, vec![1, 2, 3]), (4, vec![1, 2, 3, 4])] {
        let reader = NativeReader { fail: std::sync::atomic::AtomicU64::new(failed), calls: Mutex::new(vec![]) };
        assert!(read(&reader).is_err());
        assert_eq!(*reader.calls.lock().unwrap(), reads);
    }
    assert_eq!(reorg_ordered_checked_v2::<_, u8, ()>(&[2, 1], &[3], |_| Ok(vec![])), Ok(vec![]));
}
#[tokio::test]
async fn reorg_retry_recovers_same_notification_once_before_next_batch() {
    let reader = NativeReader { fail: std::sync::atomic::AtomicU64::new(3), calls: Mutex::new(vec![]) };
    let attempts = Cell::new(0);
    let shutdown = SingleTrigger::default();
    let started = std::time::Instant::now();
    let events = retry_reorg_logs_v2(
        || {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 2 {
                reader.fail.store(0, std::sync::atomic::Ordering::SeqCst);
            }
            std::future::ready(read(&reader))
        },
        || true,
        shutdown.listener.clone(),
        Duration::from_millis(1),
    )
    .await
    .unwrap();
    assert_eq!(attempts.get(), 2);
    assert!(started.elapsed() >= Duration::from_millis(1), "read failures must not busy-loop");
    assert_eq!(*reader.calls.lock().unwrap(), [1, 2, 3, 1, 2, 3, 4]);
    assert_eq!(
        events.iter().map(|(log, removed)| (log.data[0], *removed)).collect::<Vec<_>>(),
        [(1, true), (2, true), (3, false), (4, false)]
    );
    // Only the completed notification is returned to the broadcaster; there is
    // no detached prefix from the failed attempt to send twice.
    let next = reorg_ordered_checked_v2::<_, _, ()>(&[], &[5], |h| Ok(vec![h])).unwrap();
    assert_eq!(next, [(5, false)]);
}
#[tokio::test]
async fn reorg_retry_subscriber_departure_drops_pending_notification() {
    let wanted = Cell::new(true);
    let attempts = Cell::new(0);
    let shutdown = SingleTrigger::default();
    let result = retry_reorg_logs_v2(
        || {
            attempts.set(attempts.get() + 1);
            wanted.set(false);
            std::future::ready(Err::<Vec<u8>, _>("store"))
        },
        || wanted.get(),
        shutdown.listener,
        Duration::from_millis(1),
    )
    .await;
    assert!(result.is_none());
    assert_eq!(attempts.get(), 1);
}
#[tokio::test]
async fn reorg_retry_shutdown_cancels_read_and_delay_without_another_attempt() {
    let shutdown = SingleTrigger::default();
    let result = retry_reorg_logs_v2(
        || {
            shutdown.trigger.trigger();
            std::future::pending::<Result<Vec<u8>, &'static str>>()
        },
        || true,
        shutdown.listener.clone(),
        Duration::from_secs(60),
    )
    .await;
    assert!(result.is_none(), "a pending native read cannot prevent pump shutdown");
    let attempts = Cell::new(0);
    let shutdown = SingleTrigger::default();
    let result = retry_reorg_logs_v2(
        || {
            attempts.set(attempts.get() + 1);
            shutdown.trigger.trigger();
            std::future::ready(Err::<Vec<u8>, _>("store"))
        },
        || true,
        shutdown.listener.clone(),
        Duration::from_secs(60),
    )
    .await;
    assert!(result.is_none());
    assert_eq!(attempts.get(), 1);
}
