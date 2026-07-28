use super::utxo_set_override::{set_genesis_utxo_commitment_from_config, set_initial_utxo_set};
use super::{Consensus, ctl::Ctl};
use crate::{model::stores::U64Key, pipeline::ProcessingCounters};
use itertools::Itertools;
use kaspa_consensus_core::{api::ConsensusApi, config::Config, mining_rules::MiningRules};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensusmanager::{ConsensusFactory, ConsensusInstance, DynConsensusCtl, SessionLock};
use kaspa_core::{debug, time::unix_now, warn};
use kaspa_database::{
    prelude::{
        BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DB, DirectDbWriter, RocksDbPreset, StoreError, StoreResult,
        StoreResultExt,
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
// kaspa-pq ADR-0039/ADR-0040 PALW: bumped 7 → 8. This is the ONE cutover bump for
// PALW on-disk state uses positional bincode encoding. The following layout changes require a
// database-version transition:
//
//   1. `GhostdagData` / `CompactGhostdagData` (`model/stores/ghostdag.rs`) gained
//      `blue_hash_work` + `blue_compute_work` MID-STRUCT (before `selected_parent`).
//      These records are written for EVERY block on EVERY preset, so this break is
//      NOT confined to the PALW presets — it is why the bump is global.
//   2. `PalwBatchCertificateV2.approving_stake`, `PalwBatchLifecycleV1.
//      {cert_approving_stake,first_cert_daa}` (mid-struct) and
//      `PalwBatchViewV1.job_nullifiers` (trailing) — `consensus/core/src/palw.rs`.
//      Written on the PALW presets, which use `palw_activation_daa_score = 0`.
//
// Per ADR-0001 we reject an old-shape DB at open time (clean resync) rather than
// migrate it: `should_upgrade()` below drives `kaspad::daemon`'s 'db_upgrade loop,
// whose `version <= 13` arm requests deletion approval. That arm and this constant
// MUST move together — bumping without the arm falls through to the loop's
// `assert_eq!` and panics at startup instead of prompting.
//
// The layout-pinning tests in `consensus/core/src/palw.rs` and the version pin in
// `consensus/src/consensus/factory.rs` tests fail loudly if a future field is added
// without repeating this bump.
// kaspa-pq ADR-0040 §5.15 (ACCEPT-BIND/M2), 9 -> 10: `PalwBatchManifestV1::leaf_root` changed MEANING
// without changing its bytes. It is now a uniform-depth Merkle root (§5.15.4) rather than a flat keyed
// hash, so every `leaf_root` value moves; `leaf_root` sits inside `content_id()`, so every `batch_id`
// moves with it, and every `(batch_id, leaf_index)` leaf key and every certificate cross-bind moves
// with THAT. A version-9 datadir is therefore not short — it is semantically unrelated, which is the
// worse failure: it would decode cleanly and then fail every membership proof. Hence a hard reset, and
// hence a bump even though MANIFEST_LEN / MANIFEST_FNV are unchanged (they are pinned, and pinning them
// is what proves no field moved).
//
// SUMMARY-BIND V2, 11 -> 12: `PalwAuditorVoteV2` adds `passed_leaf_count` and
// `rejected_leaf_bitmap_root` before its signature. Certificates are persisted with bincode, so every
// non-empty `PalwBatchCertificateV2::votes` element has a new positional shape. Old rows must be
// discarded and re-synced; decoding them as V2 would fail or misread signature bytes as summary data.
//
// DA-01, 12 -> 13: prefixes 250-252 add fork-local challenge/obligation state, the canonical receipt
// object cache, and a pruning snapshot singleton. Reusing a v12 datadir would silently start those
// consensus-enforcement histories empty, allowing certificate/reward/exit checks to ignore pre-upgrade
// obligations. This is a semantic persisted-state break even though no existing row layout moved.
//
// PALW PRUNING BLOBS + DA OBJECT V2, 13 -> 14: the pruning singleton adds the canonical
// manifest/leaf/certificate projection and changes its payload version/domain. In the same cutover,
// `PalwPublicLeafV1` gains the Receipt-v3/DA-object commitment fields used by public semantic
// admission. A v13 singleton cannot reconstruct first-post-PP tickets, and its persisted leaf rows
// have the old positional shape, so both changes require one hard reset rather than a lenient decode.
//
// SEARCH-AVAILABILITY DISPATCH, 14 -> 15: prefixes 189-191 add the fork-local
// `PalwSearchAvailabilityStateV1` per-block rows, anchor links and pruning singleton (the 0x3d-0x3f
// dispatch of ADR node-anchored-web-search-da). Three positional/semantic breaks in one cutover:
// (a) every active chain block must carry a search state row/link — on a v14 datadir the
//     selected-parent loader and the reorg registry reconciler would fail-stop on the first missing
//     row rather than silently skip pre-upgrade obligations;
// (b) `PalwPruningPointSnapshotPayloadV1` inserts `search_availability_snapshot` before
//     `active_batches`, breaking the singleton's positional Borsh encoding;
// (c) `PalwSelectedParentStateV2` inserts `search_availability_state_root`, moving every Header-v4
//     overlay commitment (PALW activates via re-genesis, so this is part of that wire table).
pub const LATEST_DB_VERSION: u32 = 15;
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
        Ok(())
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

        Ok(new_entry)
    }

    pub fn commit_staging_consensus(&mut self) -> StoreResult<()> {
        self.metadata.update(DirectDbWriter::new(&self.db), |mut data| {
            assert!(data.staging_consensus_key.is_some());
            data.current_consensus_key = data.staging_consensus_key.take();
            data
        })?;
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
        // kaspa-pq (audit L-01): bake the genesis premine (10B KAS single-key ML-DSA-87
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
        };
        factory.delete_inactive_consensus_entries();
        factory
    }
}

impl ConsensusFactory for Factory {
    fn new_active_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
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
            .with_cache_budget(self.cache_budget)
            .build()
            .unwrap();

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
        ));

        // We write the new active entry only once the instance was created successfully.
        // This way we can safely avoid processing genesis in future process runs
        if is_new_consensus {
            // kaspa-pq: import the genesis premine UTXO(s) into the new consensus.
            set_initial_utxo_set(&self.config, consensus.clone(), self.config.params.genesis.hash);
            self.management_store.write().save_new_active_consensus(entry).unwrap();
        }

        (ConsensusInstance::new(session_lock, consensus.clone()), Arc::new(Ctl::new(self.management_store.clone(), db, consensus)))
    }

    fn new_staging_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        assert!(!self.notification_root.is_closed());

        let entry = self.management_store.write().new_staging_consensus_entry().unwrap();
        let dir = self.db_root_dir.join(entry.directory_name);
        let db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(dir)
            .with_parallelism(self.db_parallelism)
            .with_files_limit(self.fd_budget / 2) // active and staging consensuses should have equal budgets
            .with_preset(self.rocksdb_preset)
            .with_wal_dir(self.wal_dir.clone())
            .with_cache_budget(self.cache_budget)
            .build()
            .unwrap();

        let session_lock = SessionLock::new();
        let consensus = Arc::new(Consensus::new(
            db.clone(),
            Arc::new(self.config.to_builder().skip_adding_genesis().build()),
            session_lock.clone(),
            self.notification_root.clone(),
            self.counters.clone(),
            self.tx_script_cache_counters.clone(),
            entry.creation_timestamp,
            self.mining_rules.clone(),
        ));

        // The default for the body_missing_anticone_set is an empty vector, which corresponds precisely to the state before a consensus commit
        // But The default value for the pruning_utxoset_stable_flag is true, but a staging consensus does not have a utxo and hence the flag is dropped explicitly
        consensus.set_pruning_utxoset_stable_flag(false);

        (ConsensusInstance::new(session_lock, consensus.clone()), Arc::new(Ctl::new(self.management_store.clone(), db, consensus)))
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
    }
}

#[cfg(test)]
mod tests {
    use super::LATEST_DB_VERSION;

    /// kaspa-pq **ADR-0040 STORE-VERSION — the version pin.**
    ///
    /// [`LATEST_DB_VERSION`] identifies the on-disk format accepted by this binary. `should_upgrade()`
    /// compares it with the stored version; mismatches enter the daemon's database-upgrade flow.
    ///
    /// Changing this value is legitimate and expected when a persisted layout changes. When you do,
    /// update this pin AND the `version <= N` arm in `kaspad/src/daemon.rs` in the same change — a bump
    /// without the arm makes the loop fall through to its trailing `assert_eq!` and panic at startup,
    /// which is strictly worse than no bump at all.
    #[test]
    fn latest_db_version_is_pinned() {
        assert_eq!(
            LATEST_DB_VERSION, 15,
            "LATEST_DB_VERSION changed. If a persisted layout changed, this is correct - update this pin \
             AND extend the `version <= N` hard-reset arm in kaspad/src/daemon.rs to cover the version \
             you just left behind. Never bump one without the other."
        );
    }

    /// kaspa-pq **ADR-0040 P1-5 — the bump and the daemon arm are asserted TOGETHER.**
    ///
    /// A bump without the arm is strictly worse than no bump: `'db_upgrade` is entered, matches no arm,
    /// and trips its trailing `assert_eq!` — a startup panic with less diagnostic value than the
    /// bincode EOF it replaced. The arm lives in another crate (`kaspad`) that this one cannot import,
    /// so the coupling is checked at the source level. Reading the file is the point: the two constants
    /// have never been wrong in the same direction, only out of step.
    #[test]
    fn daemon_hard_reset_arm_covers_the_version_left_behind() {
        let daemon = include_str!("../../../kaspad/src/daemon.rs");
        let expected = format!("if version <= {} {{", LATEST_DB_VERSION - 1);
        assert!(
            daemon.contains(&expected),
            "kaspad/src/daemon.rs must hard-reset `{expected}` so a datadir at version {} takes the \
             deletion-approval arm instead of falling through to the loop's trailing assert_eq!",
            LATEST_DB_VERSION - 1
        );
        // ...and it must NOT still be the previous, now-too-narrow bound.
        assert!(
            !daemon.contains(&format!("if version <= {} {{", LATEST_DB_VERSION - 2)),
            "the stale hard-reset arm is still present; a datadir at version {} would reach the assert_eq!",
            LATEST_DB_VERSION - 1
        );
    }
}
