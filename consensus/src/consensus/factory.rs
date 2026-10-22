use super::utxo_set_override::{set_genesis_utxo_commitment_from_config, set_initial_utxo_set};
use super::{Consensus, ctl::Ctl};
use crate::{model::stores::U64Key, pipeline::ProcessingCounters};
use itertools::Itertools;
use kaspa_consensus_core::{api::ConsensusApi, config::Config, mining_rules::MiningRules};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensusmanager::{ConsensusCreationResult, ConsensusFactory, ConsensusInstance, DynConsensusCtl, SessionLock};
use kaspa_core::{debug, time::unix_now, warn};
use kaspa_database::{
    prelude::{
        BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DB, DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES, DirectDbWriter,
        RocksDbPreset, StoreError, StoreResult, StoreResultExt,
    },
    registry::DatabaseStorePrefixes,
};

use kaspa_txscript::caches::TxScriptCacheCounters;
use kaspa_utils::mem_size::MemSizeEstimator;
use parking_lot::RwLock;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fs, path::PathBuf, sync::Arc};

#[derive(Serialize, Deserialize, Clone)]
pub struct ConsensusEntry {
    key: u64,
    directory_name: String,
    creation_timestamp: u64,
}

impl MemSizeEstimator for ConsensusEntry {}

impl ConsensusEntry {
    pub fn new(key: u64, directory_name: String, creation_timestamp: u64) -> Self {
        Self { key, directory_name, creation_timestamp }
    }

    pub fn from_key(key: u64) -> Self {
        Self { key, directory_name: format!("consensus-{:0>3}", key), creation_timestamp: unix_now() }
    }
}

pub enum ConsensusEntryType {
    Existing(ConsensusEntry),
    New(ConsensusEntry),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MultiConsensusMetadata {
    current_consensus_key: Option<u64>,
    staging_consensus_key: Option<u64>,
    /// Max key used for a consensus entry
    max_key_used: u64,
    /// Memorizes whether this node was recently an archive node
    is_archival_node: bool,
    /// General serialized properties to be used cross DB versions
    props: HashMap<Vec<u8>, Vec<u8>>,
    /// The DB scheme version
    version: u32,
}

// kaspa-pq Selected-Parent EVM Lane (ADR-0020): bumped 6 → 7. The consensus
// header is bincode-serialized on disk (`database::access`), so the four new
// EVM header fields change the stored header layout; per ADR-0001 we reject an
// old-shape DB at open time (clean resync) rather than migrate it.
//
// MISAKA ADR-0038: bumped 7 → 8. `Header` gained `palw_commitment`, a `Vec<u8>`, and the header
// store is bincode over that struct — so a v7 database's rows no longer match the current layout.
// Without the bump the version check passes and the mismatch surfaces as a deserialize PANIC deep
// in an unrelated read path, which is precisely the "reject cleanly, never migrate" rule this
// constant exists to enforce (mainnet-readiness audit §5).
pub const LATEST_DB_VERSION: u32 = 8;
impl Default for MultiConsensusMetadata {
    fn default() -> Self {
        Self {
            current_consensus_key: Default::default(),
            staging_consensus_key: Default::default(),
            max_key_used: Default::default(),
            is_archival_node: Default::default(),
            props: Default::default(),
            version: LATEST_DB_VERSION,
        }
    }
}

#[derive(Clone)]
pub struct MultiConsensusManagementStore {
    db: Arc<DB>,
    entries: CachedDbAccess<U64Key, ConsensusEntry>,
    metadata: CachedDbItem<MultiConsensusMetadata>,
}

impl MultiConsensusManagementStore {
    pub fn new(db: Arc<DB>) -> Self {
        let mut store = Self {
            db: db.clone(),
            entries: CachedDbAccess::new(db.clone(), CachePolicy::Count(16), DatabaseStorePrefixes::ConsensusEntries.into()),
            metadata: CachedDbItem::new(db, DatabaseStorePrefixes::MultiConsensusMetadata.into()),
        };
        store.init();
        store
    }

    fn init(&mut self) {
        if self.metadata.read().optional().unwrap().is_none() {
            let mut batch = WriteBatch::default();
            let metadata = MultiConsensusMetadata::default();
            self.metadata.write(BatchDbWriter::new(&mut batch), &metadata).unwrap();
            self.db.write(batch).unwrap();
        }
    }

    /// The directory name of the active consensus, if one exists. None otherwise
    pub fn active_consensus_dir_name(&self) -> StoreResult<Option<String>> {
        let metadata = self.metadata.read()?;
        match metadata.current_consensus_key {
            Some(key) => Ok(Some(self.entries.read(key.into()).unwrap().directory_name)),
            None => Ok(None),
        }
    }

    /// The entry type signifies whether the returned entry is an existing/new consensus
    pub fn active_consensus_entry(&mut self) -> StoreResult<ConsensusEntryType> {
        let mut metadata = self.metadata.read()?;
        match metadata.current_consensus_key {
            Some(key) => Ok(ConsensusEntryType::Existing(self.entries.read(key.into())?)),
            None => {
                metadata.max_key_used += 1; // Capture the slot
                let key = metadata.max_key_used;
                self.metadata.write(DirectDbWriter::new(&self.db), &metadata)?;
                Ok(ConsensusEntryType::New(ConsensusEntry::from_key(key)))
            }
        }
    }

    // This function assumes metadata is already set
    pub fn staging_consensus_entry(&mut self) -> Option<ConsensusEntry> {
        let metadata = self.metadata.read().unwrap();
        match metadata.staging_consensus_key {
            Some(key) => Some(self.entries.read(key.into()).unwrap()),
            None => None,
        }
    }

    pub fn save_new_active_consensus(&mut self, entry: ConsensusEntry) -> StoreResult<()> {
        let key = entry.key;
        if self.entries.has(key.into())? {
            return Err(StoreError::KeyAlreadyExists(format!("{key}")));
        }
        let mut batch = WriteBatch::default();
        self.entries.write(BatchDbWriter::new(&mut batch), key.into(), entry)?;
        self.metadata.update(BatchDbWriter::new(&mut batch), |mut data| {
            data.current_consensus_key = Some(key);
            data
        })?;
        self.db.write(batch)?;
        self.sync_metadata()
    }

    pub fn new_staging_consensus_entry(&mut self) -> StoreResult<ConsensusEntry> {
        let mut metadata = self.metadata.read()?;

        metadata.max_key_used += 1;
        let new_key = metadata.max_key_used;
        metadata.staging_consensus_key = Some(new_key);
        let new_entry = ConsensusEntry::from_key(new_key);

        let mut batch = WriteBatch::default();
        self.metadata.write(BatchDbWriter::new(&mut batch), &metadata)?;
        self.entries.write(BatchDbWriter::new(&mut batch), new_key.into(), new_entry.clone())?;
        self.db.write(batch)?;
        // Write-ahead: the staging key is durable before its directory exists, so a crash at any later point leaves an
        // entry the next start recognises as staging and deletes (never an unreferenced directory nobody reclaims).
        self.sync_metadata()?;

        Ok(new_entry)
    }

    pub fn commit_staging_consensus(&mut self) -> StoreResult<()> {
        self.metadata.update(DirectDbWriter::new(&self.db), |mut data| {
            assert!(data.staging_consensus_key.is_some());
            data.current_consensus_key = data.staging_consensus_key.take();
            data
        })?;
        // **The switch is durable before the old consensus directory is deleted** (G14 storage, IBD lane). RocksDB's
        // default write is not synced: it survives a process kill but not an OS crash or power loss. The caller deletes
        // the previous consensus directory right after this returns, so without the fsync an OS crash could leave the
        // metadata naming the deleted directory as current — an empty "existing" consensus with no genesis.
        self.sync_metadata()
    }

    /// fsync the management DB's WAL, making every metadata write before it durable across an OS crash.
    fn sync_metadata(&self) -> StoreResult<()> {
        self.db.flush_wal(true)?;
        Ok(())
    }

    pub fn cancel_staging_consensus(&mut self) -> StoreResult<()> {
        self.metadata.update(DirectDbWriter::new(&self.db), |mut data| {
            data.staging_consensus_key = None;
            data
        })?;
        Ok(())
    }

    fn iterator(&self) -> impl Iterator<Item = Result<ConsensusEntry, Box<dyn Error>>> + '_ {
        self.entries.iterator().map(|iter_result| match iter_result {
            Ok((_, entry)) => Ok(entry),
            Err(e) => Err(e),
        })
    }

    fn iterate_inactive_entries(&self) -> impl Iterator<Item = Result<ConsensusEntry, Box<dyn Error>>> + '_ {
        let current_consensus_key = self.metadata.read().unwrap().current_consensus_key;
        self.iterator().filter(move |entry_result| {
            if let Ok(entry) = entry_result {
                return Some(entry.key) != current_consensus_key;
            }

            true
        })
    }

    fn delete_entry(&mut self, entry: ConsensusEntry) -> StoreResult<()> {
        self.entries.delete(DirectDbWriter::new(&self.db), entry.key.into())
    }

    pub fn is_archival_node(&self) -> StoreResult<bool> {
        match self.metadata.read() {
            Ok(data) => Ok(data.is_archival_node),
            Err(StoreError::KeyNotFound(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn set_is_archival_node(&mut self, is_archival_node: bool) {
        let mut metadata = self.metadata.read().unwrap();
        if metadata.is_archival_node != is_archival_node {
            metadata.is_archival_node = is_archival_node;
            let mut batch = WriteBatch::default();
            self.metadata.write(BatchDbWriter::new(&mut batch), &metadata).unwrap();
        }
    }

    /// Returns the current version of this database
    pub fn version(&self) -> StoreResult<u32> {
        match self.metadata.read() {
            Ok(data) => Ok(data.version),
            Err(err) => Err(err),
        }
    }

    /// Set the database version to a different one
    pub fn set_version(&mut self, version: u32) -> StoreResult<()> {
        self.metadata.update(DirectDbWriter::new(&self.db), |mut data| {
            data.version = version;
            data
        })?;
        Ok(())
    }

    pub fn should_upgrade(&self) -> StoreResult<bool> {
        match self.metadata.read() {
            Ok(data) => Ok(data.version != LATEST_DB_VERSION),
            Err(StoreError::KeyNotFound(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }
}

/// `consensus-NNN` directories under `root` that no entry in `known` names and whose key is below
/// `active_key` (G14: what the obsolete-consensus reclaimer may remove).
fn orphan_consensus_dirs(root: &std::path::Path, known: &[String], active_key: u64) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_prefix("consensus-")
                .and_then(|k| k.parse::<u64>().ok())
                .is_some_and(|key| key < active_key && !known.contains(&name) && entry.path().is_dir())
        })
        .map(|entry| entry.path())
        .collect()
}

pub struct Factory {
    management_store: Arc<RwLock<MultiConsensusManagementStore>>,
    config: Config,
    db_root_dir: PathBuf,
    db_parallelism: usize,
    notification_root: Arc<ConsensusNotificationRoot>,
    counters: Arc<ProcessingCounters>,
    tx_script_cache_counters: Arc<TxScriptCacheCounters>,
    fd_budget: i32,
    mining_rules: Arc<MiningRules>,
    rocksdb_preset: RocksDbPreset,
    wal_dir: Option<PathBuf>,
    cache_budget: Option<usize>,
    /// G14 capacity lane: the staging DB's storage reservation (an envelope over everything the
    /// staging DB will hold), from creation until commit or cancel. Always `None` on a node without
    /// `--storage-cap-bytes`.
    staging_reservation: parking_lot::Mutex<Option<(kaspa_utils::storage_budget::StorageReservation, PathBuf)>>,
}

impl Factory {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        management_db: Arc<DB>,
        config: &Config,
        db_root_dir: PathBuf,
        db_parallelism: usize,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        tx_script_cache_counters: Arc<TxScriptCacheCounters>,
        fd_budget: i32,
        mining_rules: Arc<MiningRules>,
        rocksdb_preset: RocksDbPreset,
        wal_dir: Option<PathBuf>,
        cache_budget: Option<usize>,
    ) -> Self {
        assert!(fd_budget > 0, "fd_budget has to be positive");
        let mut config = config.clone();
        // kaspa-pq (audit L-01): bake the genesis premine (15B KAS single-key ML-DSA-87
        // P2PKH, network-specific owner payload — see config::premine) into the genesis
        // utxo_commitment + hash for every network, so all nodes agree on the
        // premine-aware genesis identity. (Multisig/P2SH is consensus-disabled in PQ-only.)
        set_genesis_utxo_commitment_from_config(&mut config);
        config.process_genesis = false;
        let management_store = Arc::new(RwLock::new(MultiConsensusManagementStore::new(management_db)));
        management_store.write().set_is_archival_node(config.is_archival);
        let factory = Self {
            management_store,
            config,
            db_root_dir,
            db_parallelism,
            notification_root,
            counters,
            tx_script_cache_counters,
            fd_budget,
            mining_rules,
            rocksdb_preset,
            wal_dir,
            cache_budget,
            staging_reservation: parking_lot::Mutex::new(None),
        };
        factory.delete_inactive_consensus_entries();
        factory
    }

    /// **RocksDB footprint bounds**: on only when the node runs under a storage budget
    /// (`--storage-cap-bytes`); an uncapped node opens its consensus DBs exactly as before.
    fn consensus_storage_bounds(&self) -> Option<kaspa_database::prelude::StorageDbBounds> {
        kaspa_utils::storage_budget::global().map(|_| kaspa_database::prelude::StorageDbBounds::CONSENSUS)
    }

    /// Under a storage budget, a DB's WAL is fsynced when the node enters the low-space pause.
    fn register_low_space_flush(db: &Arc<DB>) {
        if let Some(budget) = kaspa_utils::storage_budget::global() {
            let weak = Arc::downgrade(db);
            budget.add_pause_hook(move || {
                if let Some(db) = weak.upgrade()
                    && let Err(e) = db.flush_wal(true)
                {
                    warn!("low-space pause: consensus WAL fsync failed: {e}");
                }
            });
        }
    }

    /// **Reclaim before use, for consensus directories** (G14 capacity lane): delete `consensus-NNN`
    /// directories no metadata entry names any more — left by a crash between a directory's removal
    /// and its entry's, or by an interrupted reset. Never a directory any entry names (active,
    /// staging, or an inactive one the factory's own post-commit / startup deletion owns — that one
    /// may still be open while a commit drops it), and never a directory whose key is not below the
    /// active key (a consensus being created right now registers its entry only once it opened).
    /// Returns the bytes freed.
    pub fn reclaim_obsolete_consensus_dirs(&self) -> u64 {
        let (known, active_key): (Vec<String>, Option<u64>) = {
            let store = self.management_store.read();
            let known = store.iterator().filter_map(|e| e.ok()).map(|e| e.directory_name).collect();
            let active_key = store.metadata.read().ok().and_then(|m| m.current_consensus_key);
            (known, active_key)
        };
        let Some(active_key) = active_key else { return 0 };
        let mut freed = 0u64;
        for path in orphan_consensus_dirs(&self.db_root_dir, &known, active_key) {
            let bytes = kaspa_utils::storage_budget::disk_usage(&path);
            match fs::remove_dir_all(&path) {
                Ok(()) => {
                    warn!(
                        "[storage] removed orphaned consensus directory {} ({bytes} bytes; no metadata entry names it)",
                        path.display()
                    );
                    freed = freed.saturating_add(bytes);
                }
                Err(e) => warn!("[storage] cannot remove orphaned consensus directory {}: {e}", path.display()),
            }
        }
        freed
    }

    /// Release the staging reservation and hand the staging directory back to the active category
    /// (after a commit it IS the active DB; after a cancel it is gone).
    fn release_staging_reservation(&self) {
        if let Some((reservation, dir)) = self.staging_reservation.lock().take() {
            if let Some(budget) = kaspa_utils::storage_budget::global() {
                budget.unassign_path(&dir);
            }
            drop(reservation);
        }
    }

    /// **The block cache a consensus database is opened with.** `--rocksdb-cache-size` (and the HDD preset's own
    /// figure) when the operator or the preset names one; otherwise, under the default preset, 256 MiB instead of
    /// RocksDB's built-in 32 MB — the PALW chain state's ~57 MB tip row makes a data block no 32 MB cache can keep, so
    /// every point read of a row beside it was a 57 MB disk read (the 2026-10-03 panel starvation,
    /// docs/design/palw/verification/t12-panel-backlog-1003.md §4). Node-local: no consensus rule and no on-disk format reads it.
    fn consensus_cache_budget(&self) -> Option<usize> {
        self.cache_budget
            .or_else(|| matches!(self.rocksdb_preset, RocksDbPreset::Default).then_some(DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES))
    }
}

impl ConsensusFactory for Factory {
    fn new_active_consensus(&self) -> ConsensusCreationResult<(ConsensusInstance, DynConsensusCtl)> {
        assert!(!self.notification_root.is_closed());

        let mut config = self.config.clone();
        let mut is_new_consensus = false;
        let entry = match self.management_store.write().active_consensus_entry().unwrap() {
            ConsensusEntryType::Existing(entry) => {
                config.process_genesis = false;
                entry
            }
            ConsensusEntryType::New(entry) => {
                // Configure to process genesis only if this is a brand new consensus
                config.process_genesis = true;
                is_new_consensus = true;
                entry
            }
        };

        let dir = self.db_root_dir.join(entry.directory_name.clone());
        let db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(dir)
            .with_parallelism(self.db_parallelism)
            .with_files_limit(self.fd_budget / 2) // active and staging consensuses should have equal budgets
            .with_preset(self.rocksdb_preset)
            .with_wal_dir(self.wal_dir.clone())
            .with_cache_budget(self.consensus_cache_budget())
            .with_storage_bounds(self.consensus_storage_bounds())
            .build()
            .unwrap();
        Self::register_low_space_flush(&db);

        let session_lock = SessionLock::new();
        let consensus = Arc::new(Consensus::new(
            db.clone(),
            Arc::new(config),
            session_lock.clone(),
            self.notification_root.clone(),
            self.counters.clone(),
            self.tx_script_cache_counters.clone(),
            entry.creation_timestamp,
            self.mining_rules.clone(),
        )?);

        // We write the new active entry only once the instance was created successfully.
        // This way we can safely avoid processing genesis in future process runs
        if is_new_consensus {
            // kaspa-pq: import the genesis premine UTXO(s) into the new consensus.
            set_initial_utxo_set(&self.config, consensus.clone(), self.config.params.genesis.hash);
            self.management_store.write().save_new_active_consensus(entry).unwrap();
        }

        Ok((ConsensusInstance::new(session_lock, consensus.clone()), Arc::new(Ctl::new(self.management_store.clone(), db, consensus))))
    }

    fn new_staging_consensus(&self) -> ConsensusCreationResult<(ConsensusInstance, DynConsensusCtl)> {
        assert!(!self.notification_root.is_closed());

        // G14 storage (IBD lane): the losing generation goes BEFORE new space is reserved or used. A staging DB whose
        // cancel could not remove its directory (or any other non-active entry) would otherwise sit beside the new one
        // until restart — three DBs where the budget holds two. (The capacity lane's staging reservation, when merged,
        // belongs right after this call.)
        self.delete_inactive_consensus_entries();

        // G14 capacity lane: under a storage budget, reserve the staging DB's whole envelope BEFORE
        // anything is created (after reclaiming obsolete data), so the staging DB and the active DB
        // never together exceed the cap. A refusal is a local capacity condition: the IBD flow
        // reports it as such and no peer or chain is condemned for it.
        let reservation = match kaspa_utils::storage_budget::global() {
            Some(budget) => {
                use kaspa_utils::storage_budget::StorageCategory;
                let bytes = budget.config().budgets[StorageCategory::StagingDb.index()];
                match budget.reserve_after_reclaim(StorageCategory::StagingDb, bytes) {
                    Ok(r) => Some(r),
                    Err(refusal) => {
                        warn!("staging consensus not created: {refusal}");
                        return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                            kaspa_utils::storage_budget::StorageReservation,
                        >("staging consensus DB storage reservation", &refusal.to_string()));
                    }
                }
            }
            None => None,
        };
        let entry = self.management_store.write().new_staging_consensus_entry().unwrap();
        let dir = self.db_root_dir.join(entry.directory_name);
        if let (Some(reservation), Some(budget)) = (reservation, kaspa_utils::storage_budget::global()) {
            budget.assign_path(dir.clone(), kaspa_utils::storage_budget::StorageCategory::StagingDb);
            *self.staging_reservation.lock() = Some((reservation, dir.clone()));
        }
        let db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(dir)
            .with_parallelism(self.db_parallelism)
            .with_files_limit(self.fd_budget / 2) // active and staging consensuses should have equal budgets
            .with_preset(self.rocksdb_preset)
            .with_wal_dir(self.wal_dir.clone())
            .with_cache_budget(self.consensus_cache_budget())
            .with_storage_bounds(self.consensus_storage_bounds())
            .build()
            .unwrap();
        Self::register_low_space_flush(&db);

        let session_lock = SessionLock::new();
        let consensus = match Consensus::new(
            db.clone(),
            Arc::new(self.config.to_builder().skip_adding_genesis().build()),
            session_lock.clone(),
            self.notification_root.clone(),
            self.counters.clone(),
            self.tx_script_cache_counters.clone(),
            entry.creation_timestamp,
            self.mining_rules.clone(),
        ) {
            Ok(consensus) => Arc::new(consensus),
            Err(unavailable) => {
                // No staging instance or worker was published. Release its DB
                // handle before the existing cancellation path removes precisely
                // the newly registered staging entry; active consensus is intact.
                drop(db);
                self.delete_staging_entry();
                return Err(unavailable);
            }
        };

        // The default for the body_missing_anticone_set is an empty vector, which corresponds precisely to the state before a consensus commit
        // But The default value for the pruning_utxoset_stable_flag is true, but a staging consensus does not have a utxo and hence the flag is dropped explicitly
        consensus.set_pruning_utxoset_stable_flag(false).unwrap();

        Ok((ConsensusInstance::new(session_lock, consensus.clone()), Arc::new(Ctl::new(self.management_store.clone(), db, consensus))))
    }

    fn close(&self) {
        debug!("Consensus factory: closing");
        self.notification_root.close();
    }

    fn delete_inactive_consensus_entries(&self) {
        // Staging entry is deleted also by archival nodes since it represents non-final data
        self.delete_staging_entry();

        if self.config.is_archival {
            return;
        }

        let mut write_guard = self.management_store.write();
        let entries_to_delete = write_guard
            .iterate_inactive_entries()
            .filter_map(|entry_result| {
                let entry = entry_result.unwrap();
                let dir = self.db_root_dir.join(entry.directory_name.clone());
                if dir.exists() {
                    match fs::remove_dir_all(dir) {
                        Ok(_) => Some(entry),
                        Err(e) => {
                            warn!("Error deleting consensus entry {}: {}", entry.key, e);
                            None
                        }
                    }
                } else {
                    Some(entry)
                }
            })
            .collect_vec();

        for entry in entries_to_delete {
            write_guard.delete_entry(entry).unwrap();
        }
        drop(write_guard);
        // Called after a commit (the staging DB is now the active one) and at startup.
        self.release_staging_reservation();
    }

    fn delete_staging_entry(&self) {
        let mut write_guard = self.management_store.write();
        if let Some(entry) = write_guard.staging_consensus_entry() {
            let dir = self.db_root_dir.join(entry.directory_name.clone());
            match fs::remove_dir_all(dir) {
                Ok(_) => {
                    write_guard.delete_entry(entry).unwrap();
                }
                Err(e) => {
                    warn!("Error deleting staging consensus entry {}: {}", entry.key, e);
                }
            };
            write_guard.cancel_staging_consensus().unwrap();
        }
        drop(write_guard);
        self.release_staging_reservation();
    }
}

#[cfg(test)]
mod g14_orphan_dir_tests {
    use super::*;

    #[test]
    fn only_unnamed_consensus_dirs_below_the_active_key_are_orphans() {
        let root = std::env::temp_dir().join(format!("g14-orphan-dirs-{}-{}", std::process::id(), unix_now()));
        for name in ["consensus-001", "consensus-002", "consensus-003", "consensus-004", "consensus-005", "not-a-consensus"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join("consensus-000"), b"a file, not a directory").unwrap();
        // 003 is active, 002 an inactive entry the factory itself deletes, 004 a staging entry, 005 being created.
        let known = vec!["consensus-002".to_string(), "consensus-003".to_string(), "consensus-004".to_string()];
        let mut orphans = orphan_consensus_dirs(&root, &known, 3);
        orphans.sort();
        assert_eq!(orphans, vec![root.join("consensus-001")]);
        fs::remove_dir_all(&root).unwrap();
    }
}
