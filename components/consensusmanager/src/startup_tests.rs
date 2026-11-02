//! The startup cycle uses the real pruning SessionLock and real proxy methods.
//! No timing sleeps decide success; polling the actual writer exposes the cycle.
use super::*;
use crate::startup::StartupGate;
use futures::{FutureExt, future::join_all};
use kaspa_consensus_core::{
    BlockHash, BlockHashMap,
    api::{BlockValidationFutures, ConsensusApi},
    block::Block,
    blockstatus::BlockStatus,
    trusted::{ExternalGhostdagData, TrustedBlock},
};
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
    time::Duration,
};

#[derive(Default)]
struct Pipeline {
    workers: AtomicBool,
    calls: Mutex<Vec<(BlockHash, bool)>>,
}
impl Pipeline {
    fn submit(&self, block: Block, trusted: bool) -> BlockValidationFutures {
        self.calls.lock().push((block.hash(), trusted));
        if self.workers.load(Ordering::Acquire) {
            BlockValidationFutures {
                block_task: async { Ok(BlockStatus::StatusHeaderOnly) }.boxed(),
                virtual_state_task: async { Ok(BlockStatus::StatusHeaderOnly) }.boxed(),
            }
        } else {
            // Precisely the old cycle: queued jobs cannot finish until startup
            // returns from its writer and starts the block workers.
            BlockValidationFutures {
                block_task: futures::future::pending().boxed(),
                virtual_state_task: futures::future::pending().boxed(),
            }
        }
    }
}
impl ConsensusApi for Pipeline {
    fn get_sink(&self) -> BlockHash {
        BlockHash::from_bytes([42; 64])
    }
    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
        self.submit(block, false)
    }
    fn validate_and_insert_trusted_block(&self, block: TrustedBlock) -> BlockValidationFutures {
        self.submit(block.block, true)
    }
}
fn block(id: u8, parent: Option<u8>) -> Block {
    Block::from_precomputed_hash(BlockHash::from_bytes([id; 64]), parent.into_iter().map(|p| BlockHash::from_bytes([p; 64])).collect())
}
fn trusted(id: u8) -> TrustedBlock {
    TrustedBlock::new(
        block(id, None),
        ExternalGhostdagData {
            blue_score: 0,
            blue_work: 0.into(),
            selected_parent: BlockHash::from_bytes([0; 64]),
            mergeset_blues: vec![],
            mergeset_reds: vec![],
            blues_anticone_sizes: BlockHashMap::default(),
        },
    )
}
fn initializing() -> (ConsensusInstance, SessionLock, Arc<Pipeline>) {
    let lock = SessionLock::new();
    let pipeline = Arc::new(Pipeline::default());
    let instance = ConsensusInstance::new_initializing(lock.clone(), pipeline.clone());
    (instance, lock, pipeline)
}
fn start(instance: &ConsensusInstance, pipeline: &Pipeline) {
    pipeline.workers.store(true, Ordering::Release);
    instance.finish_startup();
}

#[tokio::test]
async fn startup_gate_recovery_writer_finishes_before_inbound_sync_then_sync_succeeds() {
    let (instance, lock, pipeline) = initializing();
    let sync = async {
        let session = instance.session().await;
        session.validate_and_insert_block(block(1, None)).virtual_state_task.await.unwrap()
    };
    futures::pin_mut!(sync);
    assert!(matches!(futures::poll!(&mut sync), Poll::Pending));
    assert!(pipeline.calls.lock().is_empty(), "inbound sync must not queue before workers exist");
    let recovery_writer = lock.0.write();
    futures::pin_mut!(recovery_writer);
    let recovery_guard = match futures::poll!(&mut recovery_writer) {
        Poll::Ready(guard) => guard,
        Poll::Pending => panic!("inbound sync took a pruning read guard before recovery could finish"),
    };
    // Publication/pruning complete under the actual write guard; only after
    // releasing it may the controller finish worker startup and admit readers.
    drop(recovery_guard);
    start(&instance, &pipeline);
    assert_eq!(tokio::time::timeout(Duration::from_secs(5), sync).await.unwrap(), BlockStatus::StatusHeaderOnly);
    assert_eq!(pipeline.calls.lock().as_slice(), &[(BlockHash::from_bytes([1; 64]), false)]);
}

#[tokio::test]
async fn startup_gate_blocking_session_waits_without_pruning_read_guard() {
    let (instance, lock, pipeline) = initializing();
    let session = instance.session_blocking();
    futures::pin_mut!(session);
    assert!(matches!(futures::poll!(&mut session), Poll::Pending));
    let writer = lock.0.write();
    futures::pin_mut!(writer);
    assert!(matches!(futures::poll!(&mut writer), Poll::Ready(_)));
    start(&instance, &pipeline);
    drop(session.await);
}

#[tokio::test]
async fn startup_gate_unguarded_block_waits_and_both_tasks_share_one_insertion() {
    let (instance, _, pipeline) = initializing();
    let tasks = instance.unguarded_session().validate_and_insert_block(block(2, None));
    let mut body = tasks.block_task;
    let mut virtual_state = tasks.virtual_state_task;
    assert!(matches!(futures::poll!(&mut body), Poll::Pending));
    assert!(matches!(futures::poll!(&mut virtual_state), Poll::Pending));
    assert!(pipeline.calls.lock().is_empty());
    start(&instance, &pipeline);
    assert!(body.await.is_ok());
    assert!(virtual_state.await.is_ok());
    assert_eq!(pipeline.calls.lock().len(), 1);
}

#[tokio::test]
async fn startup_gate_virtual_only_trusted_submission_is_not_lost_or_duplicated() {
    let (instance, _, pipeline) = initializing();
    let tasks = instance.unguarded_session().validate_and_insert_trusted_block(trusted(3));
    drop(tasks.block_task);
    let mut virtual_state = tasks.virtual_state_task;
    assert!(matches!(futures::poll!(&mut virtual_state), Poll::Pending));
    assert!(pipeline.calls.lock().is_empty());
    start(&instance, &pipeline);
    assert!(virtual_state.await.is_ok());
    assert_eq!(pipeline.calls.lock().as_slice(), &[(BlockHash::from_bytes([3; 64]), true)]);
}

#[tokio::test]
async fn startup_gate_blocking_unguarded_submission_obeys_gate() {
    let (instance, _, pipeline) = initializing();
    let tasks = instance.unguarded_session_blocking().validate_and_insert_block(block(4, None));
    let mut body = tasks.block_task;
    assert!(matches!(futures::poll!(&mut body), Poll::Pending));
    assert!(pipeline.calls.lock().is_empty());
    start(&instance, &pipeline);
    assert!(body.await.is_ok());
    assert_eq!(pipeline.calls.lock().len(), 1);
}

#[tokio::test]
async fn startup_gate_batch_queues_once_in_parent_order_even_when_last_future_is_polled_first() {
    let (instance, _, pipeline) = initializing();
    let mut batch = instance.unguarded_session().validate_and_insert_block_batch(vec![block(6, Some(5)), block(5, None)]);
    let bodies = batch.block_tasks.take().unwrap();
    let mut virtuals = batch.virtual_state_tasks.take().unwrap();
    let mut last = virtuals.pop().unwrap();
    assert!(matches!(futures::poll!(&mut last), Poll::Pending));
    assert!(pipeline.calls.lock().is_empty());
    start(&instance, &pipeline);
    assert!(last.await.is_ok());
    assert!(join_all(virtuals).await.into_iter().all(|r| r.is_ok()));
    assert!(join_all(bodies).await.into_iter().all(|r| r.is_ok()));
    assert_eq!(pipeline.calls.lock().as_slice(), &[(BlockHash::from_bytes([5; 64]), false), (BlockHash::from_bytes([6; 64]), false)]);
}

#[tokio::test]
async fn startup_gate_cancelling_before_ready_neither_holds_reader_nor_inserts_later() {
    let (instance, lock, pipeline) = initializing();
    let tasks = instance.unguarded_session().validate_and_insert_block(block(7, None));
    let mut body = tasks.block_task;
    assert!(matches!(futures::poll!(&mut body), Poll::Pending));
    drop(body);
    drop(tasks.virtual_state_task);
    {
        let session = instance.session();
        futures::pin_mut!(session);
        assert!(matches!(futures::poll!(&mut session), Poll::Pending));
    }
    let writer = lock.0.write();
    futures::pin_mut!(writer);
    assert!(matches!(futures::poll!(&mut writer), Poll::Ready(_)));
    start(&instance, &pipeline);
    assert!(pipeline.calls.lock().is_empty());
}

#[tokio::test]
async fn startup_gate_ready_constructor_preserves_eager_block_trusted_and_batch_submission() {
    let pipeline = Arc::new(Pipeline::default());
    pipeline.workers.store(true, Ordering::Release);
    let instance = ConsensusInstance::new(SessionLock::new(), pipeline.clone());
    let session = instance.unguarded_session();
    drop(session.validate_and_insert_block(block(8, None)));
    drop(session.validate_and_insert_trusted_block(trusted(9)));
    drop(session.validate_and_insert_block_batch(vec![block(10, None), block(11, None)]));
    assert_eq!(pipeline.calls.lock().len(), 4, "ready calls enqueue eagerly without polling their futures");
}

#[tokio::test]
async fn startup_gate_finishing_before_wait_or_during_registration_never_loses_wakeup() {
    let gate = Arc::new(StartupGate::new(false));
    gate.finish();
    tokio::time::timeout(Duration::from_secs(5), gate.wait()).await.unwrap();
    let registered = StartupGate::new(false);
    let waiting = registered.wait();
    futures::pin_mut!(waiting);
    assert!(matches!(futures::poll!(&mut waiting), Poll::Pending));
    registered.finish();
    tokio::time::timeout(Duration::from_secs(5), waiting).await.unwrap();
    for _ in 0..128 {
        let gate = Arc::new(StartupGate::new(false));
        let worker = gate.clone();
        let waiting = tokio::spawn(async move { worker.wait().await });
        gate.finish();
        tokio::time::timeout(Duration::from_secs(5), waiting).await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn startup_gate_unguarded_metadata_remains_available_during_recovery() {
    let (instance, _, _) = initializing();
    assert_eq!(instance.unguarded_session_blocking().get_sink(), BlockHash::from_bytes([42; 64]));
    assert_eq!(instance.unguarded_session().spawn_blocking(|api| api.get_sink()).await, BlockHash::from_bytes([42; 64]));
}

#[tokio::test]
async fn startup_gate_regression_control_ready_legacy_session_reproduces_writer_cycle() {
    let lock = SessionLock::new();
    let pipeline = Arc::new(Pipeline::default());
    let instance = ConsensusInstance::new(lock.clone(), pipeline.clone());
    {
        let sync = async {
            let session = instance.session().await;
            session.validate_and_insert_block(block(12, None)).virtual_state_task.await
        };
        futures::pin_mut!(sync);
        assert!(matches!(futures::poll!(&mut sync), Poll::Pending));
        assert_eq!(pipeline.calls.lock().len(), 1);
        let writer = lock.0.write();
        futures::pin_mut!(writer);
        assert!(
            matches!(futures::poll!(&mut writer), Poll::Pending),
            "legacy reader excludes startup's writer while awaiting absent workers"
        );
    }
    let writer = lock.0.write();
    futures::pin_mut!(writer);
    assert!(matches!(futures::poll!(&mut writer), Poll::Ready(_)), "cancelling the legacy flow releases the actual cycle");
}
