use crate::core::model::{CompactUtxoCollection, CompactUtxoEntry, UtxoSetByScriptPublicKey};

use kaspa_consensus_core::tx::{
    ScriptPublicKey, ScriptPublicKeyVersion, ScriptPublicKeys, ScriptVec, TransactionIndexType, TransactionOutpoint,
};
use kaspa_core::debug;
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_index_core::indexed_utxos::BalanceByScriptPublicKey;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

pub const VERSION_TYPE_SIZE: usize = size_of::<ScriptPublicKeyVersion>(); // Const since we need to re-use this a few times.

/// [`ScriptPublicKeyBucket`].
/// Consists of 2 bytes of little endian [VersionType] bytes, followed by the script length (8) and by a variable size of [ScriptVec].
#[derive(Eq, Hash, PartialEq, Debug, Clone)]
struct ScriptPublicKeyBucket(Vec<u8>);

impl From<&ScriptPublicKey> for ScriptPublicKeyBucket {
    fn from(script_public_key: &ScriptPublicKey) -> Self {
        // version (2) + length (8) + dynamic script
        let mut bytes: Vec<u8> = Vec::with_capacity(VERSION_TYPE_SIZE + size_of::<u64>() + script_public_key.script().len());
        bytes.extend_from_slice(&script_public_key.version().to_le_bytes());
        bytes.extend_from_slice(&(script_public_key.script().len() as u64).to_le_bytes()); // TODO: Consider using a smaller integer
        bytes.extend_from_slice(script_public_key.script());
        Self(bytes)
    }
}

impl From<ScriptPublicKeyBucket> for ScriptPublicKey {
    fn from(bucket: ScriptPublicKeyBucket) -> Self {
        let version = ScriptPublicKeyVersion::from_le_bytes(
            <[u8; VERSION_TYPE_SIZE]>::try_from(&bucket.0[..VERSION_TYPE_SIZE]).expect("expected version size"),
        );

        let script_size =
            u64::from_le_bytes(bucket.0[VERSION_TYPE_SIZE..VERSION_TYPE_SIZE + size_of::<u64>()].try_into().unwrap()) as usize;
        let script =
            ScriptVec::from_slice(&bucket.0[VERSION_TYPE_SIZE + size_of::<u64>()..VERSION_TYPE_SIZE + size_of::<u64>() + script_size]);

        Self::new(version, script)
    }
}

impl AsRef<[u8]> for ScriptPublicKeyBucket {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

// Keys:

// TransactionOutpoint:
// PR-9.5c: TransactionId widened from `Hash` (32 B) to `Hash64`
// (64 B). The DB key layout widens from 36 to 68 bytes; existing
// upstream-Kaspa utxoindex databases become unreadable, matching
// ADR-0001's "kaspa-pq is a new network, no DB migration" rule.
/// Size of the [TransactionOutpointKey] in bytes.
pub const TRANSACTION_OUTPOINT_KEY_SIZE: usize = kaspa_hashes::HASH64_SIZE + size_of::<TransactionIndexType>();

/// [TransactionOutpoint] key which references the [CompactUtxoEntry] within a [ScriptPublicKeyBucket]
/// Consists of 64 bytes of [TransactionId] (Hash64), followed by 4 bytes of little endian [TransactionIndexType]
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct TransactionOutpointKey([u8; TRANSACTION_OUTPOINT_KEY_SIZE]);

impl From<TransactionOutpointKey> for TransactionOutpoint {
    fn from(key: TransactionOutpointKey) -> Self {
        let transaction_id = kaspa_hashes::Hash64::from_slice(&key.0[..kaspa_hashes::HASH64_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; size_of::<TransactionIndexType>()]>::try_from(&key.0[kaspa_hashes::HASH64_SIZE..]).expect("expected index size"),
        );
        Self::new(transaction_id, index)
    }
}

impl From<&TransactionOutpoint> for TransactionOutpointKey {
    fn from(outpoint: &TransactionOutpoint) -> Self {
        let mut bytes = [0; TRANSACTION_OUTPOINT_KEY_SIZE];
        bytes[..kaspa_hashes::HASH64_SIZE].copy_from_slice(&outpoint.transaction_id.as_bytes());
        bytes[kaspa_hashes::HASH64_SIZE..].copy_from_slice(&outpoint.index.to_le_bytes());
        Self(bytes)
    }
}

impl AsRef<[u8]> for TransactionOutpointKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Full [CompactUtxoEntry] access key.
/// Consists of variable amount of bytes of [ScriptPublicKeyBucket], and 36 bytes of [TransactionOutpointKey]
#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize)]
struct UtxoEntryFullAccessKey(Arc<Vec<u8>>);

impl Display for UtxoEntryFullAccessKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self) // TODO: Deserialize first
    }
}

impl UtxoEntryFullAccessKey {
    /// Creates a new [UtxoEntryFullAccessKey] from a [ScriptPublicKeyBucket] and [TransactionOutpointKey].
    pub fn new(script_public_key_bucket: ScriptPublicKeyBucket, transaction_outpoint_key: TransactionOutpointKey) -> Self {
        let mut bytes = Vec::with_capacity(script_public_key_bucket.as_ref().len() + TRANSACTION_OUTPOINT_KEY_SIZE);
        bytes.extend_from_slice(script_public_key_bucket.as_ref());
        bytes.extend_from_slice(transaction_outpoint_key.as_ref());
        Self(Arc::new(bytes))
    }

    pub fn extract_outpoint(&self) -> TransactionOutpoint {
        TransactionOutpoint::from(TransactionOutpointKey(self.0[(self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE)..].try_into().unwrap()))
    }
}

impl AsRef<[u8]> for UtxoEntryFullAccessKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

// CachedDbAccess exposes boxed iterator errors. Preserve actual RocksDB I/O
// failures; malformed serialized rows remain explicit consistency errors.
fn indexed_utxo_read_error(why: Box<dyn std::error::Error>) -> StoreError {
    match why.downcast::<rocksdb::Error>() {
        Ok(why) => StoreError::DbError(*why),
        Err(why) => StoreError::DataInconsistency(format!("UTXO index iterator: {why}")),
    }
}

// Traits:

pub trait UtxoSetByScriptPublicKeyStoreReader {
    /// Get [UtxoSetByScriptPublicKey] set by queried [ScriptPublicKeys],
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;
    /// Stop before retaining entry max_entries + 1, across every queried script.
    fn get_utxos_from_script_public_keys_bounded(
        &self,
        script_public_keys: ScriptPublicKeys,
        max_entries: usize,
    ) -> StoreResult<UtxoSetByScriptPublicKey>;
    /// Streaming count for generation admission; does not materialize a map.
    fn count_utxos_from_script_public_keys_bounded(
        &self,
        script_public_keys: &ScriptPublicKeys,
        max_entries: usize,
    ) -> StoreResult<usize>;
    /// Paginated single-script variant of [Self::get_utxos_from_script_public_keys]: returns up to
    /// `chunk_size` UTXOs for `script_public_key` starting after the opaque `cursor` (the raw outpoint
    /// key of the previous page's last entry), plus the cursor to resume from when more entries remain
    /// (`None` = no further pages). Bounds the per-call memory/response footprint for addresses with a
    /// very large UTXO set (e.g. an unconsolidated mining payout).
    fn get_utxos_from_script_public_key_chunk(
        &self,
        script_public_key: &ScriptPublicKey,
        cursor: Option<&[u8]>,
        chunk_size: usize,
    ) -> StoreResult<(UtxoSetByScriptPublicKey, Option<Vec<u8>>)>;
    fn get_balance_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey>;
    fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>>; // This can have a big memory footprint, so it should be used only for tests.
}

pub trait UtxoSetByScriptPublicKeyStore: UtxoSetByScriptPublicKeyStoreReader {
    /// remove [UtxoSetByScriptPublicKey] from the [UtxoSetByScriptPublicKeyStore].
    fn remove_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()>;

    /// add [UtxoSetByScriptPublicKey] into the [UtxoSetByScriptPublicKeyStore].
    fn add_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()>;

    /// removes all entries in the cache and db, besides prefixes themselves.
    fn delete_all(&mut self) -> StoreResult<()>;
}

// Implementations:

#[derive(Clone)]
pub struct DbUtxoSetByScriptPublicKeyStore {
    db: Arc<DB>,
    access: CachedDbAccess<UtxoEntryFullAccessKey, CompactUtxoEntry>,
}

impl DbUtxoSetByScriptPublicKeyStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoIndex.into()) }
    }
}

impl UtxoSetByScriptPublicKeyStoreReader for DbUtxoSetByScriptPublicKeyStore {
    // compared to go-kaspad this gets transaction outpoints from multiple script public keys at once.
    // TODO: probably ideal way to retrieve is to return a chained iterator which can be used to chunk results and propagate utxo entries
    // to the rpc via pagination, this would alleviate the memory footprint of script public keys with large amount of utxos.
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        self.get_utxos_from_script_public_keys_bounded(script_public_keys, usize::MAX)
    }

    fn get_utxos_from_script_public_keys_bounded(
        &self,
        script_public_keys: ScriptPublicKeys,
        max_entries: usize,
    ) -> StoreResult<UtxoSetByScriptPublicKey> {
        let script_count = script_public_keys.len();
        let mut entries_count = 0usize;
        let mut result = UtxoSetByScriptPublicKey::new();
        for script in script_public_keys {
            let bucket = ScriptPublicKeyBucket::from(&script);
            let mut collection = CompactUtxoCollection::new();
            for entry in self.access.seek_iterator(
                Some(bucket.as_ref()),
                None,
                max_entries.saturating_sub(entries_count).saturating_add(1),
                false,
            ) {
                let (key, value) = entry.map_err(indexed_utxo_read_error)?;
                if entries_count == max_entries {
                    return Err(StoreError::LocalResourceUnavailable(format!(
                        "UTXO query exceeds the {max_entries} entry cap across {script_count} scripts; use getUtxosByAddressPage (cursor-paginated) or getBalancesByAddresses (balance only)"
                    )));
                }
                let key: [u8; TRANSACTION_OUTPOINT_KEY_SIZE] = key
                    .as_ref()
                    .try_into()
                    .map_err(|_| StoreError::DataInconsistency("UTXO index outpoint key has invalid length".into()))?;
                collection.insert(TransactionOutpointKey(key).into(), value);
                entries_count += 1;
            }
            result.insert(script, collection);
        }
        debug!("IDXPRC, Executed a bounded query for {script_count} script public keys yielding {entries_count} entries");
        Ok(result)
    }

    fn count_utxos_from_script_public_keys_bounded(
        &self,
        script_public_keys: &ScriptPublicKeys,
        max_entries: usize,
    ) -> StoreResult<usize> {
        let mut count = 0usize;
        for script in script_public_keys {
            let bucket = ScriptPublicKeyBucket::from(script);
            for row in
                self.access.seek_iterator(Some(bucket.as_ref()), None, max_entries.saturating_sub(count).saturating_add(1), false)
            {
                let (key, _) = row.map_err(indexed_utxo_read_error)?;
                if key.len() != TRANSACTION_OUTPOINT_KEY_SIZE {
                    return Err(StoreError::DataInconsistency("UTXO index outpoint key has invalid length".into()));
                }
                if count == max_entries {
                    return Err(StoreError::LocalResourceUnavailable(format!(
                        "UTXO query exceeds the {max_entries} entry cap; use getUtxosByAddressPage or getBalancesByAddresses"
                    )));
                }
                count += 1;
            }
        }
        Ok(count)
    }

    fn get_utxos_from_script_public_key_chunk(
        &self,
        script_public_key: &ScriptPublicKey,
        cursor: Option<&[u8]>,
        chunk_size: usize,
    ) -> StoreResult<(UtxoSetByScriptPublicKey, Option<Vec<u8>>)> {
        if chunk_size == 0 {
            return Err(StoreError::LocalResourceUnavailable("UTXO page size must be positive".into()));
        }
        let script_public_key_bucket = ScriptPublicKeyBucket::from(script_public_key);
        let cursor = cursor.filter(|c| c.len() == TRANSACTION_OUTPOINT_KEY_SIZE);
        let seek_from = cursor.map(|c| {
            UtxoEntryFullAccessKey::new(
                script_public_key_bucket.clone(),
                TransactionOutpointKey(c.try_into().expect("validated cursor length")),
            )
        });
        let mut items = Vec::new();
        // Seek inclusively, then skip only an equal cursor. An unconditional
        // skip loses the next live UTXO when the previous page's last UTXO was
        // spent/deleted between calls.
        for entry in self.access.seek_iterator(Some(script_public_key_bucket.as_ref()), seek_from, chunk_size.saturating_add(2), false)
        {
            let (key, value) = entry.map_err(indexed_utxo_read_error)?;
            if cursor.is_some_and(|c| key.as_ref() <= c) {
                continue;
            }
            if key.len() != TRANSACTION_OUTPOINT_KEY_SIZE {
                return Err(StoreError::DataInconsistency("UTXO index outpoint key has invalid length".into()));
            }
            items.push((key.as_ref().to_vec(), value));
            if items.len() > chunk_size {
                break;
            }
        }
        let next_cursor = if items.len() > chunk_size {
            items.pop(); // lookahead is not part of this page
            // Resume after the last RETURNED row, not the discarded lookahead.
            items.last().map(|(key, _)| key.clone())
        } else {
            None
        };
        let collection = CompactUtxoCollection::from_iter(items.into_iter().map(|(key, entry)| {
            (TransactionOutpointKey(<[u8; TRANSACTION_OUTPOINT_KEY_SIZE]>::try_from(&key[..]).unwrap()).into(), entry)
        }));
        let mut result = UtxoSetByScriptPublicKey::new();
        result.insert(script_public_key.clone(), collection);
        Ok((result, next_cursor))
    }

    fn get_balance_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey> {
        let script_count = script_public_keys.len();
        let mut entries_count: usize = 0;
        let mut balance_by_script_public_keys = BalanceByScriptPublicKey::new();
        for script_public_key in script_public_keys.into_iter() {
            let script_public_key_bucket = ScriptPublicKeyBucket::from(&script_public_key);
            let balance: u64 = self
                .access
                .seek_iterator(Some(script_public_key_bucket.as_ref()), None, usize::MAX, false)
                .map(|res| {
                    entries_count += 1;
                    let (_, entry) = res.unwrap();
                    entry.amount
                })
                .sum();
            balance_by_script_public_keys.insert(script_public_key, balance);
        }
        debug!("IDXPRC, Executed a query for the balance of {} script public keys involving {} entries", script_count, entries_count);
        Ok(balance_by_script_public_keys)
    }

    // This can have a big memory footprint, so it should be used only for tests.
    fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>> {
        Ok(HashSet::from_iter(
            self.access.iterator().map(|res| UtxoEntryFullAccessKey(Arc::new(res.unwrap().0.to_vec())).extract_outpoint()),
        ))
    }
}

impl UtxoSetByScriptPublicKeyStore for DbUtxoSetByScriptPublicKeyStore {
    fn remove_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut writer = DirectDbWriter::new(&self.db);

        let mut to_remove = utxo_entries.iter().flat_map(move |(script_public_key, compact_utxo_collection)| {
            compact_utxo_collection.keys().map(move |transaction_outpoint| {
                UtxoEntryFullAccessKey::new(
                    ScriptPublicKeyBucket::from(script_public_key),
                    TransactionOutpointKey::from(transaction_outpoint),
                )
            })
        });

        self.access.delete_many(&mut writer, &mut to_remove)?;

        Ok(())
    }

    fn add_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut writer = DirectDbWriter::new(&self.db);

        let mut to_add = utxo_entries.iter().flat_map(move |(script_public_key, compact_utxo_collection)| {
            compact_utxo_collection.iter().map(move |(transaction_outpoint, compact_utxo)| {
                (
                    UtxoEntryFullAccessKey::new(
                        ScriptPublicKeyBucket::from(script_public_key),
                        TransactionOutpointKey::from(transaction_outpoint),
                    ),
                    *compact_utxo,
                )
            })
        });

        self.access.write_many(&mut writer, &mut to_add)?;

        Ok(())
    }

    /// Removes all entries in the cache and db, besides prefixes themselves.
    fn delete_all(&mut self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }
}

#[cfg(test)]
mod bounded_query_tests {
    use super::*;
    use kaspa_database::{
        create_temp_db,
        prelude::{BatchDbWriter, ConnBuilder},
    };
    use std::collections::HashMap;

    fn script(tag: u8) -> ScriptPublicKey {
        ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51, tag]))
    }
    fn outpoint(index: u64) -> TransactionOutpoint {
        TransactionOutpoint::new(kaspa_hashes::Hash64::from_u64_word(index), 0)
    }
    fn add_rows(db: &Arc<DB>, script: &ScriptPublicKey, start: u64, count: usize) {
        let bucket = ScriptPublicKeyBucket::from(script);
        let mut batch = rocksdb::WriteBatch::default();
        let access: CachedDbAccess<UtxoEntryFullAccessKey, CompactUtxoEntry> =
            CachedDbAccess::new(db.clone(), CachePolicy::Empty, DatabaseStorePrefixes::UtxoIndex.into());
        for index in start..start + count as u64 {
            let key = UtxoEntryFullAccessKey::new(bucket.clone(), TransactionOutpointKey::from(&outpoint(index)));
            access.write(BatchDbWriter::new(&mut batch), key, CompactUtxoEntry::new(index + 1, index, false)).unwrap();
        }
        db.write(batch).unwrap();
    }
    fn normalized(set: &UtxoSetByScriptPublicKey) -> HashMap<(ScriptPublicKey, TransactionOutpoint), (u64, u64, bool)> {
        set.iter()
            .flat_map(|(script, rows)| {
                rows.iter().map(move |(key, value)| ((script.clone(), *key), (value.amount, value.block_daa_score, value.is_coinbase)))
            })
            .collect()
    }
    fn scripts(items: &[ScriptPublicKey]) -> ScriptPublicKeys {
        items.iter().cloned().collect()
    }

    #[test]
    fn reserved_generation_pressure_child() {
        if std::env::var_os("MISAKA_UTXO_PRESSURE_CHILD").is_none() {
            return;
        }
        use kaspa_core::memory_budget::{MemoryClass, global_memory_budget};
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let a = script(1);
        let b = script(2);
        add_rows(&db, &a, 0, 2);
        add_rows(&db, &b, 20, 1);
        let store = crate::stores::store_manager::Store::new(db.clone());
        let query = scripts(&[a, b]);
        let budget = global_memory_budget();
        let total = budget.snapshot().limit_bytes;
        let snapshot_cap = total.saturating_sub((total / 8).min(128 << 20));
        let held = budget.try_reserve(MemoryClass::Snapshot, snapshot_cap).unwrap();
        match store.get_utxos_by_script_public_key_reserved(query.clone(), 3, 1024, 4096) {
            Err(StoreError::LocalResourceUnavailable(message)) => assert!(message.contains("byte budget"), "{message}"),
            _ => panic!("DB count fits, but generation must refuse before returning a materialized map"),
        }
        assert_eq!(budget.snapshot().reserved_bytes, snapshot_cap, "failed admission did not leak another ticket");
        let control = budget.try_reserve(MemoryClass::Transport, 1024).unwrap();
        control.materialized();
        drop(control);
        drop(held);
        let (entries, memory) = store.get_utxos_by_script_public_key_reserved(query.clone(), 3, 1024, 4096).unwrap();
        assert_eq!(entries.values().map(|rows| rows.len()).sum::<usize>(), 3);
        assert_eq!(memory.bytes(), 3 * 1024 + 4096);
        assert_eq!(budget.snapshot().pending_bytes, memory.bytes());
        memory.materialized();
        assert_eq!(budget.snapshot().pending_bytes, 0);
        let last = memory.clone();
        drop(memory);
        drop(entries);
        assert_eq!(budget.snapshot().reserved_bytes, last.bytes(), "last owner still covers transport handoff");
        drop(last);
        assert_eq!(budget.snapshot().reserved_bytes, 0);
        assert!(store.get_utxos_by_script_public_key_reserved(query, 3, u64::MAX, 4096).is_err());
        assert_eq!(budget.snapshot().reserved_bytes, 0, "overflow refuses without a reservation");
        std::fs::write(std::env::var_os("MISAKA_UTXO_PRESSURE_RESULT").unwrap(), b"passed").unwrap();
    }

    #[test]
    fn generation_admission_failure_and_last_owner_release_make_progress() {
        let marker = std::env::temp_dir().join(format!(
            "misaka-utxo-pressure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "stores::indexed_utxos::bounded_query_tests::reserved_generation_pressure_child", "--nocapture"])
            .env("MISAKA_UTXO_PRESSURE_CHILD", "1")
            .env("MISAKA_UTXO_PRESSURE_RESULT", &marker)
            .env("MISAKA_MEMORY_BUDGET_BYTES", (64u64 << 20).to_string())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read(&marker).unwrap(), b"passed", "child filter must execute its assertions");
        std::fs::remove_file(marker).unwrap();
    }

    #[test]
    fn bounded_query_matches_legacy_below_cap_and_counts_all_addresses() {
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let a = script(1);
        let b = script(2);
        let empty = script(3);
        add_rows(&db, &a, 0, 3);
        add_rows(&db, &b, 10, 4);
        let query = scripts(&[a, b, empty]);
        let legacy = store.get_utxos_from_script_public_keys(query.clone()).unwrap();
        let bounded = store.get_utxos_from_script_public_keys_bounded(query.clone(), 7).unwrap();
        assert_eq!(normalized(&legacy), normalized(&bounded));
        assert!(matches!(store.get_utxos_from_script_public_keys_bounded(query, 6), Err(StoreError::LocalResourceUnavailable(_))));
    }

    #[test]
    fn legacy_rpc_cap_boundary_accepts_250000_and_refuses_250001_before_conversion() {
        const CAP: usize = 250_000;
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let a = script(1);
        add_rows(&db, &a, 0, CAP);
        let result = store.get_utxos_from_script_public_keys_bounded(scripts(&[a.clone()]), CAP).unwrap();
        assert_eq!(result[&a].len(), CAP);
        drop(result);
        add_rows(&db, &a, CAP as u64, 1);
        assert!(matches!(
            store.get_utxos_from_script_public_keys_bounded(scripts(&[a]), CAP),
            Err(StoreError::LocalResourceUnavailable(_))
        ));
    }

    #[test]
    fn all_pages_equal_legacy_including_size_one_and_cursor_is_last_returned_row() {
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let a = script(1);
        add_rows(&db, &a, 0, 8);
        let expected = normalized(&store.get_utxos_from_script_public_keys(scripts(&[a.clone()])).unwrap());
        for size in [1, 2, 3, 8, 9] {
            let mut seen = HashMap::new();
            let mut cursor = None;
            let mut calls = 0;
            loop {
                let (page, next) = store.get_utxos_from_script_public_key_chunk(&a, cursor.as_deref(), size).unwrap();
                let page = normalized(&page);
                assert!(page.len() <= size);
                if let Some(next) = &next {
                    let last = page.keys().map(|(_, key)| TransactionOutpointKey::from(key).as_ref().to_vec()).max().unwrap();
                    assert_eq!(*next, last, "cursor must not name the discarded lookahead row");
                }
                for (key, value) in page {
                    assert!(seen.insert(key, value).is_none(), "no duplicate UTXO across pages");
                }
                calls += 1;
                assert!(calls <= 9, "cursor must progress");
                cursor = next;
                if cursor.is_none() {
                    break;
                }
            }
            assert_eq!(seen, expected, "page size {size}");
        }
        let malformed = store.get_utxos_from_script_public_key_chunk(&a, Some(&[1, 2, 3]), 1).unwrap();
        let initial = store.get_utxos_from_script_public_key_chunk(&a, None, 1).unwrap();
        assert_eq!(normalized(&malformed.0), normalized(&initial.0));
        assert!(store.get_utxos_from_script_public_key_chunk(&a, None, 0).is_err());
    }

    #[test]
    fn removed_cursor_does_not_skip_the_next_live_row_and_read_errors_propagate() {
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let a = script(1);
        add_rows(&db, &a, 0, 5);
        let (_, cursor) = store.get_utxos_from_script_public_key_chunk(&a, None, 2).unwrap();
        let cursor = cursor.unwrap();
        let key = UtxoEntryFullAccessKey::new(
            ScriptPublicKeyBucket::from(&a),
            TransactionOutpointKey(cursor.as_slice().try_into().unwrap()),
        );
        db.delete([DatabaseStorePrefixes::UtxoIndex.as_ref(), key.as_ref()].concat()).unwrap();
        let expected = normalized(&store.get_utxos_from_script_public_keys(scripts(&[a.clone()])).unwrap())
            .into_iter()
            .filter(|((_, key), _)| TransactionOutpointKey::from(key).as_ref() > cursor.as_slice())
            .collect::<HashMap<_, _>>();
        let page = store.get_utxos_from_script_public_key_chunk(&a, Some(&cursor), 10).unwrap();
        assert_eq!(normalized(&page.0), expected);
        // Corrupt storage must return an error, never an empty success/panic.
        let badkey = UtxoEntryFullAccessKey::new(ScriptPublicKeyBucket::from(&a), TransactionOutpointKey::from(&outpoint(99)));
        db.put([DatabaseStorePrefixes::UtxoIndex.as_ref(), badkey.as_ref()].concat(), [0xff]).unwrap();
        assert!(store.get_utxos_from_script_public_keys_bounded(scripts(&[a.clone()]), 100).is_err());
        assert!(store.get_utxos_from_script_public_key_chunk(&a, None, 100).is_err());
        let read_error =
            rocksdb::DB::open_for_read_only(&rocksdb::Options::default(), db.path().join("missing-read-only-database"), false)
                .err()
                .expect("missing read-only DB supplies a real RocksDB error");
        assert!(matches!(indexed_utxo_read_error(Box::new(read_error)), StoreError::DbError(_)));
    }
}
