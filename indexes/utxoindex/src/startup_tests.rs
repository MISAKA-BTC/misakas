//! Real initializing ConsensusManager/SessionLock and real RocksDB index store.
//! The controlled metadata API never starts or accepts a block-processing job.
use super::*;
use futures::FutureExt;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_consensusmanager::{
    ConsensusCreationResult, ConsensusCtl, ConsensusFactory, ConsensusInstance, DynConsensusCtl, SessionLock,
};
use kaspa_core::{core::Core, service::Service};
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[derive(Default)]
struct Metadata {
    reads: AtomicUsize,
}
impl Metadata {
    fn tips() -> BlockHashSet {
        [BlockHash::from_bytes([7; 64])].into_iter().collect()
    }
    fn utxos() -> Vec<(TransactionOutpoint, UtxoEntry)> {
        [17, 29]
            .into_iter()
            .enumerate()
            .map(|(index, amount)| {
                (
                    TransactionOutpoint::new(BlockHash::from_bytes([8; 64]), index as u32),
                    UtxoEntry::new(amount, ScriptPublicKey::new(0, Default::default()), 3, false),
                )
            })
            .collect()
    }
}
impl ConsensusApi for Metadata {
    fn get_virtual_parents(&self) -> BlockHashSet {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Self::tips()
    }
    fn get_virtual_utxos(
        &self,
        from: Option<TransactionOutpoint>,
        _chunk: usize,
        _skip: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        assert!(from.is_none(), "tiny fixture is one metadata page");
        Self::utxos()
    }
}
struct Controller {
    lock: SessionLock,
    started: Arc<AtomicBool>,
}
impl ConsensusCtl for Controller {
    fn start(&self) -> Vec<thread::JoinHandle<()>> {
        // The actual recovery writer must still be able to finish after index
        // construction and before manager startup opens ordinary sessions.
        let _writer = self.lock.blocking_write();
        self.started.store(true, Ordering::SeqCst);
        vec![]
    }
    fn stop(&self) {}
    fn make_active(&self) {}
}
struct Factory {
    api: Arc<Metadata>,
    lock: SessionLock,
    started: Arc<AtomicBool>,
}
impl ConsensusFactory for Factory {
    fn new_active_consensus(&self) -> ConsensusCreationResult<(ConsensusInstance, DynConsensusCtl)> {
        Ok((
            ConsensusInstance::new_initializing(self.lock.clone(), self.api.clone()),
            Arc::new(Controller { lock: self.lock.clone(), started: self.started.clone() }),
        ))
    }
    fn new_staging_consensus(&self) -> ConsensusCreationResult<(ConsensusInstance, DynConsensusCtl)> {
        unreachable!("this fixture never adopts a staging instance")
    }
    fn close(&self) {}
    fn delete_inactive_consensus_entries(&self) {}
    fn delete_staging_entry(&self) {}
}
fn initializing() -> (Arc<ConsensusManager>, Arc<Metadata>, SessionLock, Arc<AtomicBool>) {
    let api = Arc::new(Metadata::default());
    let lock = SessionLock::new();
    let started = Arc::new(AtomicBool::new(false));
    let manager =
        Arc::new(ConsensusManager::new(Arc::new(Factory { api: api.clone(), lock: lock.clone(), started: started.clone() })).unwrap());
    (manager, api, lock, started)
}
fn start(manager: &Arc<ConsensusManager>) {
    for handle in manager.clone().start(Arc::new(Core::new())) {
        handle.join().unwrap();
    }
}
fn assert_index(index: &Arc<RwLock<UtxoIndex>>) {
    let index = index.read();
    assert_eq!(index.get_circulating_supply().unwrap(), 46);
    assert_eq!(*index.get_utxo_index_tips().unwrap(), Metadata::tips());
    assert_eq!(index.get_all_outpoints().unwrap(), Metadata::utxos().into_iter().map(|(outpoint, _)| outpoint).collect());
}

#[test]
fn startup_index_constructor_finishes_before_initializing_manager_workers() {
    let (manager, api, _lock, started) = initializing();
    let instance = manager.consensus();
    assert!(instance.session().now_or_never().is_none());
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (tx, rx) = mpsc::channel();
    let constructing = manager.clone();
    let worker = thread::spawn(move || tx.send(UtxoIndex::new_before_processors(constructing, db).unwrap()).unwrap());
    let result = rx.recv_timeout(Duration::from_secs(5));
    if result.is_err() {
        // Unblock a regressed constructor before reporting the failed test.
        start(&manager);
        worker.join().unwrap();
        panic!("bootstrap index waited for the not-yet-started manager");
    }
    let index = result.unwrap();
    worker.join().unwrap();
    assert!(!started.load(Ordering::SeqCst));
    assert!(api.reads.load(Ordering::SeqCst) >= 2);
    assert_index(&index);
    assert!(instance.session().now_or_never().is_none(), "bootstrap metadata must not open the inbound gate");
    start(&manager);
    assert!(started.load(Ordering::SeqCst));
    assert!(index.read().is_synced().unwrap());
}

#[test]
fn startup_index_ordinary_constructor_still_waits_for_processors() {
    let (manager, api, _lock, started) = initializing();
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    let constructing = manager.clone();
    let worker = thread::spawn(move || {
        entered_tx.send(()).unwrap();
        tx.send(UtxoIndex::new(constructing, db).unwrap()).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // This is a blocking-semantics control, not a speed assertion.
    let held = rx.recv_timeout(Duration::from_millis(100));
    assert!(held.is_err());
    assert_eq!(api.reads.load(Ordering::SeqCst), 0);
    assert!(!started.load(Ordering::SeqCst));
    start(&manager);
    let index = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert_index(&index);
}

#[test]
fn startup_index_runtime_resync_retains_pruning_guard() {
    let (manager, api, lock, _started) = initializing();
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let index = UtxoIndex::new_before_processors(manager.clone(), db).unwrap();
    start(&manager);
    let writer = lock.blocking_write();
    let reads = api.reads.load(Ordering::SeqCst);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    let resyncing = index.clone();
    let worker = thread::spawn(move || {
        entered_tx.send(()).unwrap();
        resyncing.write().resync().unwrap();
        tx.send(()).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let held = rx.recv_timeout(Duration::from_millis(100));
    assert!(held.is_err());
    assert_eq!(api.reads.load(Ordering::SeqCst), reads, "runtime metadata must wait for the writer");
    drop(writer);
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert_index(&index);
    assert!(index.read().is_synced().unwrap());
}
