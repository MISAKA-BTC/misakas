//! A single simulated consensus node: a real `Consensus` on a temp DB with its processors running.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::runtime::Runtime;

use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::dns_finality::DnsState;
use kaspa_consensus_core::mining_rules::MiningRules;
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_database::create_temp_db;
use kaspa_database::prelude::ConnBuilder;
use kaspa_database::utils::DbLifetime;

use crate::config::Config;
use crate::consensus::Consensus;
use crate::model::stores::dns_state::DnsStateStoreReader;
use kaspa_consensus_core::BlockHashSet;

/// One node of the simulated network. Owns its DB lifetime so the temp dir outlives the run.
///
/// The processor join handles sit behind a mutex so the node can be shut down through a shared
/// `Arc` (the actors hold clones for the whole run).
pub(super) struct SimNode {
    pub(super) consensus: Arc<Consensus>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    /// A current-thread runtime used purely to await the block-processing futures synchronously
    /// (simpa uses `futures::executor::block_on`; this crate does not depend on `futures`). The
    /// futures are channel-backed and need no reactor.
    rt: Runtime,
    _db: DbLifetime,
}

impl SimNode {
    /// Mirrors `simpa`'s node construction (temp DB + dummy notifier + `run_processors`) and
    /// `TestConsensus::new`'s argument shape (creation_timestamp 0 — wall-clock must not leak into
    /// a virtual-time run).
    pub(super) fn new(config: Arc<Config>) -> Self {
        let (db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (dummy_notification_sender, _) = async_channel::unbounded();
        let notification_root = Arc::new(ConsensusNotificationRoot::new(dummy_notification_sender));
        let consensus = Arc::new(Consensus::new(
            db,
            config,
            Default::default(),
            notification_root,
            Default::default(),
            Default::default(),
            0,
            Arc::new(MiningRules::default()),
        ));
        let handles = consensus.run_processors();
        let rt = tokio::runtime::Builder::new_current_thread().build().expect("current-thread runtime");
        Self { consensus, handles: Mutex::new(handles), rt, _db: db_lifetime }
    }

    /// Inserts a block and blocks on its virtual-state resolution (simpa's miner pattern), so the
    /// node's view is fully settled before the simulation advances to the next event.
    pub(super) fn insert(&self, block: Block) {
        let session = self.consensus.acquire_session();
        let status = self.rt.block_on(self.consensus.validate_and_insert_block(block).virtual_state_task).unwrap();
        assert!(status.is_utxo_valid_or_pending(), "simulated block must reach a UTXO-valid-or-pending status");
        drop(session);
    }

    /// Inserts a block that may already be known (a node hears the same block via several links);
    /// duplicates are silently ignored.
    pub(super) fn insert_if_new(&self, block: Block, seen: &mut BlockHashSet) {
        if seen.insert(block.header.hash) {
            self.insert(block);
        }
    }

    pub(super) fn sink(&self) -> kaspa_consensus_core::BlockHash {
        self.consensus.get_sink()
    }

    pub(super) fn dns_state(&self) -> DnsState {
        self.consensus.storage.dns_state_store.read().get().expect("DnsState is initialized at genesis")
    }

    pub(super) fn shutdown(&self) {
        let handles = std::mem::take(&mut *self.handles.lock().unwrap());
        if !handles.is_empty() {
            self.consensus.shutdown(handles);
        }
    }
}

impl Drop for SimNode {
    /// Shutdown-on-drop guard: an assert failing mid-run must still join the processor threads
    /// BEFORE `_db` drops, otherwise `DbLifetime::drop` panics over the still-referenced DB and
    /// the double panic aborts the whole test process (taking parallel tests with it).
    fn drop(&mut self) {
        self.shutdown();
    }
}
