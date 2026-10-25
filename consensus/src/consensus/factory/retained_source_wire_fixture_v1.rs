//! cfg(test)-only prepared-fixture constructor and observations.
//! Normal Factory::new must retain its canonical-genesis validation unchanged.
use super::*;
use kaspa_consensus_core::{
    muhash::MuHashExtensions,
    tx::{TransactionOutpoint, UtxoEntry},
};
use std::sync::{
    Weak,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

/// Not an alternate ConsensusFactory: actual Factory methods create active and
/// staging instances and actual Ctl performs durable publication.
pub(crate) struct FixtureClockV1 {
    now: AtomicU64,
    instances: parking_lot::Mutex<Vec<Weak<Consensus>>>,
    staging_created: AtomicUsize,
}
impl FixtureClockV1 {
    pub(crate) fn new(now: u64) -> Arc<Self> {
        assert!(now > 0);
        Arc::new(Self { now: AtomicU64::new(now), instances: Default::default(), staging_created: AtomicUsize::new(0) })
    }
    fn arm(consensus: &Consensus, now: u64) {
        let old = consensus.header_processor.validation_clock.fetch_max(now, Ordering::Relaxed);
        consensus.virtual_processor.set_template_time_for_test(now.max(old));
        assert!(consensus.header_processor.validation_clock.load(Ordering::Relaxed) != 0);
    }
    pub(super) fn register(&self, consensus: &Arc<Consensus>, staging: bool) {
        Self::arm(consensus, self.now.load(Ordering::Relaxed));
        self.instances.lock().push(Arc::downgrade(consensus));
        if staging {
            self.staging_created.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(crate) fn advance(&self, now: u64) {
        let previous = self.now.fetch_max(now, Ordering::Relaxed);
        for consensus in self.instances.lock().iter().filter_map(Weak::upgrade) {
            Self::arm(&consensus, now.max(previous));
        }
    }
    pub(crate) fn staging_created(&self) -> usize {
        self.staging_created.load(Ordering::Relaxed)
    }
}

impl Factory {
    /// Requires a seeded *existing* management entry and the exact known private
    /// fixture Config/premine already rederived and fingerprinted by the caller.
    /// Available only to consensus unit tests, not Args or feature-built nodes.
    pub(crate) fn retained_wire_fixture_v1(
        management_db: Arc<DB>,
        config: &Config,
        db_root: PathBuf,
        notification_root: Arc<ConsensusNotificationRoot>,
        premine: &[(TransactionOutpoint, UtxoEntry)],
        expected_genesis: kaspa_consensus_core::BlockHash,
        expected_params_id: &str,
        clock: Arc<FixtureClockV1>,
    ) -> Arc<Self> {
        assert_eq!(config.params.genesis.hash, expected_genesis);
        assert_eq!(config.params.consensus_params_id().to_string(), expected_params_id);
        let mut multiset = kaspa_muhash::MuHash::new();
        for (outpoint, entry) in premine {
            multiset.add_utxo(outpoint, entry);
        }
        assert_eq!(
            multiset.finalize(),
            config.params.genesis.utxo_commitment,
            "exact existing fixture premine, no invented allocation"
        );
        assert_eq!(kaspa_consensus_core::header::Header::from(&config.params.genesis).hash, expected_genesis);
        let mut prepared = config.clone();
        prepared.process_genesis = false;
        // The common initializer follows the unchanged canonical M-07 guard in
        // Factory::new; this exact test fixture has its own premine check above.
        let mut factory = Self::from_prepared_config_v1(
            management_db,
            prepared,
            db_root,
            1,
            notification_root,
            Default::default(),
            Default::default(),
            80,
            Arc::new(MiningRules::default()),
            RocksDbPreset::Default,
            None,
            Some(DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES),
        );
        let directory = factory.management_store.read().active_consensus_dir_name().unwrap().expect("existing-only fixture bootstrap");
        assert!(factory.db_root_dir.join(directory).join("CURRENT").is_file());
        // cfg(test)-only field, default None for normal Factory::new; actual
        // new_active/new_staging register before processors start.
        factory.retained_fixture_clock_v1 = Some(clock);
        Arc::new(factory)
    }

    pub(crate) fn retained_fixture_active_directory_v1(&self) -> String {
        self.management_store.read().active_consensus_dir_name().unwrap().expect("real active management entry")
    }

    pub(crate) fn retained_fixture_active_consensus_v1(&self) -> Arc<Consensus> {
        let path = self.db_root_dir.join(self.retained_fixture_active_directory_v1());
        let clock = self.retained_fixture_clock_v1.as_ref().expect("test-only retained fixture registry");
        clock
            .instances
            .lock()
            .iter()
            .filter_map(Weak::upgrade)
            .find(|consensus| consensus.db.path() == path)
            .expect("active management entry names an actual Factory-created native instance")
    }

    pub(super) fn register_retained_fixture_instance_v1(&self, consensus: &Arc<Consensus>, staging: bool) {
        if let Some(clock) = &self.retained_fixture_clock_v1 {
            clock.register(consensus, staging);
        }
    }
}

/// Reserve and durably publish a real Existing management entry before seeding
/// only that node's scratch DB. No handwritten max key or opaque prefix copies.
pub(crate) fn reserve_existing_entry_v1(management_db: Arc<DB>, root: &PathBuf) -> PathBuf {
    let mut management = MultiConsensusManagementStore::new(management_db);
    let entry = match management.active_consensus_entry().unwrap() {
        ConsensusEntryType::New(entry) => entry,
        ConsensusEntryType::Existing(_) => panic!("wire fixture management must be fresh"),
    };
    management.save_new_active_consensus(entry).unwrap();
    root.join(management.active_consensus_dir_name().unwrap().unwrap())
}
