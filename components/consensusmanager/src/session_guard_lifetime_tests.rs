//! Actual session helper/lock ownership. Timeouts only bound a broken harness;
//! lock readiness is measured by polling the real writer, never elapsed sleep.
use super::*;
use std::{sync::mpsc, task::Poll, time::Duration};
use tokio::{sync::oneshot, task::JoinHandle};

const BOUND: Duration = Duration::from_secs(5);

struct Reader;
impl ConsensusApi for Reader {
    fn get_sink(&self) -> BlockHash {
        BlockHash::from_bytes([7; 64])
    }
}

/// Even assertion failure or cancellation releases the real worker's latch.
struct ReleaseOnDrop(Option<mpsc::Sender<()>>);
impl ReleaseOnDrop {
    fn release(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.release();
    }
}
struct Finished(Option<oneshot::Sender<()>>);
impl Drop for Finished {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}
struct Running {
    lock: SessionLock,
    caller: JoinHandle<u64>,
    release: ReleaseOnDrop,
    finished: oneshot::Receiver<()>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.caller.abort();
        self.release.release();
    }
}
async fn started(panic: bool) -> Running {
    let lock = SessionLock::default();
    let session = ConsensusSessionOwned::new(lock.read_owned().await, Arc::new(Reader));
    let (entered_tx, entered_rx) = oneshot::channel();
    let (finished_tx, finished) = oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let caller = tokio::spawn(session.spawn_blocking(move |c| {
        let _finished = Finished(Some(finished_tx));
        entered_tx.send(()).unwrap();
        release_rx.recv().expect("test owns the worker's release latch");
        // Real API dispatch after the caller may have been cancelled proves the
        // closure actually continues with its captured consensus owner.
        assert_eq!(c.get_sink(), BlockHash::from_bytes([7; 64]));
        if panic {
            panic!("intentional session worker unwind");
        }
        42
    }));
    let running = Running { lock, caller, release: ReleaseOnDrop(Some(release_tx)), finished };
    tokio::time::timeout(BOUND, entered_rx).await.expect("worker must enter").unwrap();
    running
}
async fn writer_ready(lock: &SessionLock) -> bool {
    let writer = lock.0.write();
    futures::pin_mut!(writer);
    matches!(futures::poll!(&mut writer), Poll::Ready(_))
}
async fn finished_and_writer_released(running: &mut Running) {
    tokio::time::timeout(BOUND, &mut running.finished).await.expect("worker must leave its latch").unwrap();
    // The finished signal is sent by closure cleanup; the helper's capture can
    // drop just after it. Await the actual lock to establish final release.
    let writer = tokio::time::timeout(BOUND, running.lock.0.write()).await.expect("finished helper must release its guard");
    drop(writer);
}

#[tokio::test]
async fn session_guard_lifetime_normal_return_keeps_writer_out_until_worker_finishes() {
    let mut running = started(false).await;
    let before = writer_ready(&running.lock).await;
    running.release.release();
    let value = tokio::time::timeout(BOUND, &mut running.caller).await.unwrap().unwrap();
    finished_and_writer_released(&mut running).await;
    assert!(!before, "actual worker must exclude the actual writer");
    assert_eq!(value, 42);
    assert!(writer_ready(&running.lock).await);
}

#[tokio::test]
async fn session_guard_lifetime_cancelled_awaiter_cannot_release_running_worker_guard() {
    let mut running = started(false).await;
    let before = writer_ready(&running.lock).await;
    running.caller.abort();
    let cancelled = tokio::time::timeout(BOUND, &mut running.caller).await.unwrap().unwrap_err();
    assert!(cancelled.is_cancelled(), "abort completion establishes caller future was dropped");
    // Worker cannot leave its real synchronous latch yet. A Ready writer is
    // therefore the defect, rather than an inference from a scheduling delay.
    let after_cancel = writer_ready(&running.lock).await;
    running.release.release();
    finished_and_writer_released(&mut running).await;
    // Assert after cleanup so the intentionally red baseline leaves no blocked
    // Tokio blocking worker behind when the assertion fails.
    assert!(!before);
    assert!(!after_cancel, "cancelling the awaiter released its session guard while its blocking worker was still running");
}

#[tokio::test]
async fn session_guard_lifetime_worker_panic_releases_guard_on_unwind() {
    let mut running = started(true).await;
    let before = writer_ready(&running.lock).await;
    running.release.release();
    let error = tokio::time::timeout(BOUND, &mut running.caller).await.unwrap().unwrap_err();
    finished_and_writer_released(&mut running).await;
    assert!(!before);
    assert!(error.is_panic(), "the actual helper propagates its blocking JoinError panic");
    assert!(writer_ready(&running.lock).await);
}
