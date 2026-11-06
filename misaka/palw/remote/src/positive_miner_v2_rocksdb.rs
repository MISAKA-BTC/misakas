//! Dedicated, irreversible A2 local preparation anchors backed by synchronous
//! RocksDB CAS. This physical database is NOT a consensus/native-state database.
//! It has no rollback, prune, delete, native-row, payment or credit API.
//!
//! Only a fresh authenticated MinerResponder can bind a case. Its result,
//! service and process limits remain immutable even before its first anchor.
//! Local I/O expenditure survives native branch rollback; reorg owners must
//! never restore an earlier copy of this database as native selected-chain state.
//! The configured parent directory must be trusted and protected against
//! replacement by other principals. This adapter checks the root UID/mode and
//! singleton ownership marker UID/mode/link count, but does not audit ancestors.
//! The operator protects this directory: losing or rolling back BOTH the local
//! journal and this protected database cannot be detected from those files alone.

use super::{
    recovery::{JournalAnchorV2, ProtectedAnchorStoreV2},
    *,
};
use ::rocksdb::{BlockBasedOptions, Cache, DBWithThreadMode, MultiThreaded, Options, WriteBatch, WriteOptions};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
};

type LocalDb = DBWithThreadMode<MultiThreaded>;
const OWNER_FILE: &str = "MISAKA-A2-ANCHORS.OWNER";
const OWNER_BYTES: &[u8] = b"MISAKA dedicated A2 local anchors v1\n";
const SCHEMA_KEY: &[u8] = b"schema/v1";
const COUNT_KEY: &[u8] = b"case-count/v1";
const CASE_PREFIX: &[u8] = b"case/v1/";
const SCHEMA_MAGIC: &[u8; 8] = b"A2PDBV1\0";
const ROW_MAGIC: &[u8; 8] = b"A2ANCHR1";
const SCHEMA_BYTES: usize = 8 + 5 * 8 + 4;
const CASE_KEY_BYTES: usize = CASE_PREFIX.len() + 3 * 64;
const CASE_ROW_BYTES: usize = 8 + 2 * 64 + 5 * 8 + 2 * (1 + 8 + 64);
const FIXED_LOGICAL_BYTES: u64 = (SCHEMA_KEY.len() + SCHEMA_BYTES + COUNT_KEY.len() + 8) as u64;
const CASE_LOGICAL_BYTES: u64 = (CASE_KEY_BYTES + CASE_ROW_BYTES) as u64;
const MIB: usize = 1 << 20;

/// Logical key/value quotas are exact for this fixed-row layout. RocksDB cache,
/// memtable, FD, WAL, compaction, MANIFEST and retained-log settings are finite;
/// these settings are not a physical disk/RSS quota or a performance measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RocksAnchorDbLimitsV2 {
    pub max_cases: u64,
    pub max_sequence: u64,
    pub max_logical_bytes: u64,
    pub write_buffer_bytes: usize,
    pub block_cache_bytes: usize,
    pub max_open_files: i32,
}
impl RocksAnchorDbLimitsV2 {
    fn validate(self) -> Result<(), MinerErrorV2> {
        if self.max_cases == 0
            || self.max_sequence == 0
            || self.max_logical_bytes < FIXED_LOGICAL_BYTES
            || self.max_cases.checked_mul(CASE_LOGICAL_BYTES).and_then(|n| n.checked_add(FIXED_LOGICAL_BYTES)).is_none()
            || !(MIB..=64 * MIB).contains(&self.write_buffer_bytes)
            || !(MIB..=64 * MIB).contains(&self.block_cache_bytes)
            || !(16..=128).contains(&self.max_open_files)
        {
            return Err(MinerErrorV2::Budget);
        }
        Ok(())
    }
    fn within(self, original: Self) -> bool {
        self.max_cases <= original.max_cases
            && self.max_sequence <= original.max_sequence
            && self.max_logical_bytes <= original.max_logical_bytes
            && self.write_buffer_bytes <= original.write_buffer_bytes
            && self.block_cache_bytes <= original.block_cache_bytes
            && self.max_open_files <= original.max_open_files
    }
}
struct Owner {
    db: LocalDb,
    limits: RocksAnchorDbLimitsV2,
    // Every handle is derived from this private owner. Independently opening the
    // same physical DB is rejected by RocksDB's cross-process exclusive LOCK.
    writer: Mutex<WriterState>,
}

#[derive(Default)]
struct WriterState {
    active_cases: BTreeSet<[u8; CASE_KEY_BYTES]>,
}

/// One private physical DB owner. Clones and all per-case handles share ONE
/// writer mutex. Per-case handles are non-Clone exclusive leases until Drop;
/// this prevents identical-CAS forks through two different journal paths. No raw
/// DB handle, native namespace or rollback path is exposed.
#[derive(Clone)]
pub struct RocksAnchorDbV2 {
    owner: Arc<Owner>,
}
pub struct RocksCaseAnchorV2 {
    owner: Arc<Owner>,
    key: [u8; CASE_KEY_BYTES],
    binding: CaseBinding,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CaseBinding {
    result: Digest,
    service: Digest,
    process_limits: [u64; 5],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CaseRow {
    binding: CaseBinding,
    current: Option<JournalAnchorV2>,
    previous: Option<JournalAnchorV2>,
}

impl RocksAnchorDbV2 {
    /// Explicit new-store operation. Existing directories (including empty
    /// ones) are refused, so this cannot silently recreate a disappeared store
    /// or attach its keys to a native database supplied by mistake.
    pub fn create(path: &Path, limits: RocksAnchorDbLimitsV2) -> Result<Self, MinerErrorV2> {
        limits.validate()?;
        create_private_directory(path)?;
        let mut file = owner_options(true).open(path.join(OWNER_FILE)).map_err(io_error)?;
        file.write_all(OWNER_BYTES).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        sync_directory(path)?;
        sync_parent(path)?;
        let db = LocalDb::open(&db_options(limits, true), path).map_err(db_error)?;
        let mut batch = WriteBatch::default();
        batch.put(SCHEMA_KEY, encode_schema(limits));
        batch.put(COUNT_KEY, 0u64.to_le_bytes());
        write_sync(&db, batch)?;
        sync_directory(path)?;
        Ok(Self { owner: Arc::new(Owner { db, limits, writer: Mutex::new(WriterState::default()) }) })
    }

    /// Reopen an existing owned store; absence/unknown format is an error.
    /// Limits may be lowered from the creation policy, never enlarged. Stored
    /// counts, bindings, anchors and per-case process limits are not reset.
    pub fn open(path: &Path, limits: RocksAnchorDbLimitsV2) -> Result<Self, MinerErrorV2> {
        limits.validate()?;
        require_private_directory(path)?;
        check_owner_file(path)?;
        let db = LocalDb::open(&db_options(limits, false), path).map_err(db_error)?;
        let marker = db.get_pinned(SCHEMA_KEY).map_err(db_error)?.ok_or_else(|| error("missing local anchor schema"))?;
        let original = decode_schema(&marker)?;
        if !limits.within(original) {
            return Err(MinerErrorV2::Budget);
        }
        let count = read_count(&db)?;
        check_capacity(limits, count)?;
        drop(marker);
        Ok(Self { owner: Arc::new(Owner { db, limits, writer: Mutex::new(WriterState::default()) }) })
    }

    /// Bind only AFTER result, service, admission and signer authentication by
    /// MinerResponder::new. A raw peer DTO cannot occupy this immutable slot.
    /// Registering a new slot and incrementing count share one sync WriteBatch.
    pub fn bind<D>(&self, responder: &MinerResponderV2<'_, D>) -> Result<RocksCaseAnchorV2, MinerErrorV2> {
        if responder.poisoned || responder.meter != PreparationMeterV2::default() || !responder.prepared.is_empty() {
            return Err(error("local anchor binding requires a fresh authenticated responder"));
        }
        let binding = CaseBinding {
            result: responder.result.id().0,
            service: responder.contract.id(),
            process_limits: process_limits(responder.limits),
        };
        if responder.result.service_contract != binding.service || responder.contract.case != responder.result.case_id() {
            return Err(MinerErrorV2::Binding);
        }
        let key = case_key(&responder.result);
        let mut guard = self.owner.writer.lock().map_err(|_| error("local anchor writer lock poisoned"))?;
        if guard.active_cases.contains(&key) {
            return Err(error("local anchor case already has an active writer"));
        }
        let current = read_case(&self.owner.db, &key)?;
        if let Some(row) = current {
            if row.binding != binding {
                return Err(MinerErrorV2::Binding);
            }
            check_sequence(self.owner.limits, row.current)?;
        } else {
            let count = read_count(&self.owner.db)?.checked_add(1).ok_or(MinerErrorV2::Budget)?;
            check_capacity(self.owner.limits, count)?;
            let row = CaseRow { binding, current: None, previous: None };
            let mut batch = WriteBatch::default();
            batch.put(key, encode_case(row));
            batch.put(COUNT_KEY, count.to_le_bytes());
            write_sync(&self.owner.db, batch)?;
        }
        guard.active_cases.insert(key);
        Ok(RocksCaseAnchorV2 { owner: self.owner.clone(), key, binding })
    }

    pub fn case_count(&self) -> Result<u64, MinerErrorV2> {
        let _guard = self.owner.writer.lock().map_err(|_| error("local anchor writer lock poisoned"))?;
        let count = read_count(&self.owner.db)?;
        check_capacity(self.owner.limits, count)?;
        Ok(count)
    }
    pub fn logical_bytes(&self) -> Result<u64, MinerErrorV2> {
        logical_bytes(self.case_count()?)
    }
}
impl ProtectedAnchorStoreV2 for RocksCaseAnchorV2 {
    fn load(&self) -> Result<Option<JournalAnchorV2>, MinerErrorV2> {
        let _guard = self.owner.writer.lock().map_err(|_| error("local anchor writer lock poisoned"))?;
        if !_guard.active_cases.contains(&self.key) {
            return Err(error("local anchor case writer lease missing"));
        }
        let row = read_case(&self.owner.db, &self.key)?.ok_or_else(|| error("bound local anchor row disappeared"))?;
        if row.binding != self.binding {
            return Err(MinerErrorV2::Binding);
        }
        check_sequence(self.owner.limits, row.current)?;
        Ok(row.current)
    }
    fn commit(&mut self, expected: Option<JournalAnchorV2>, next: JournalAnchorV2) -> Result<(), MinerErrorV2> {
        let _guard = self.owner.writer.lock().map_err(|_| error("local anchor writer lock poisoned"))?;
        if !_guard.active_cases.contains(&self.key) {
            return Err(error("local anchor case writer lease missing"));
        }
        let row = read_case(&self.owner.db, &self.key)?.ok_or_else(|| error("bound local anchor row disappeared"))?;
        if row.binding != self.binding {
            return Err(MinerErrorV2::Binding);
        }
        let required_sequence = expected.map(|old| old.sequence.checked_add(1).ok_or(MinerErrorV2::Budget)).transpose()?.unwrap_or(0);
        if next.sequence != required_sequence || next.sequence > self.owner.limits.max_sequence {
            return Err(MinerErrorV2::Budget);
        }
        // Retry after an uncertain successful sync is idempotent only for the
        // exact previous AND next identity. It never recreates an older row.
        if row.current == Some(next) && row.previous == expected {
            return Ok(());
        }
        if row.current != expected {
            return Err(error("local anchor compare-and-commit conflict"));
        }
        let mut batch = WriteBatch::default();
        batch.put(self.key, encode_case(CaseRow { binding: self.binding, current: Some(next), previous: expected }));
        write_sync(&self.owner.db, batch)
    }
}

impl Drop for RocksCaseAnchorV2 {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.owner.writer.lock() {
            guard.active_cases.remove(&self.key);
        }
    }
}

fn case_key(result: &MinerResultCommit) -> [u8; CASE_KEY_BYTES] {
    let mut key = [0u8; CASE_KEY_BYTES];
    key[..CASE_PREFIX.len()].copy_from_slice(CASE_PREFIX);
    let offset = CASE_PREFIX.len();
    key[offset..offset + 64].copy_from_slice(&result.scope.network.0);
    key[offset + 64..offset + 128].copy_from_slice(&result.miner.0.0);
    key[offset + 128..].copy_from_slice(&result.case_id().0.0);
    key
}

fn process_limits(limits: ProcessLimitsV2) -> [u64; 5] {
    [
        limits.max_units_per_request,
        limits.max_tracked_units,
        limits.max_provider_reads,
        limits.max_prepared_bytes,
        limits.max_status_checks,
    ]
}
fn logical_bytes(count: u64) -> Result<u64, MinerErrorV2> {
    count.checked_mul(CASE_LOGICAL_BYTES).and_then(|n| n.checked_add(FIXED_LOGICAL_BYTES)).ok_or(MinerErrorV2::Budget)
}
fn check_capacity(limits: RocksAnchorDbLimitsV2, count: u64) -> Result<(), MinerErrorV2> {
    if count > limits.max_cases || logical_bytes(count)? > limits.max_logical_bytes {
        return Err(MinerErrorV2::Budget);
    }
    Ok(())
}
fn check_sequence(limits: RocksAnchorDbLimitsV2, head: Option<JournalAnchorV2>) -> Result<(), MinerErrorV2> {
    if head.is_some_and(|head| head.sequence > limits.max_sequence) {
        return Err(MinerErrorV2::Budget);
    }
    Ok(())
}
fn read_count(db: &LocalDb) -> Result<u64, MinerErrorV2> {
    let bytes = db.get_pinned(COUNT_KEY).map_err(db_error)?.ok_or_else(|| error("local anchor case count missing"))?;
    if bytes.len() != 8 {
        return Err(error("bad local anchor case count"));
    }
    Ok(u64::from_le_bytes(bytes.as_ref().try_into().expect("8")))
}
fn read_case(db: &LocalDb, key: &[u8]) -> Result<Option<CaseRow>, MinerErrorV2> {
    let Some(bytes) = db.get_pinned(key).map_err(db_error)? else { return Ok(None) };
    // Check the pinned length before copying/decoding any potentially corrupt value.
    if bytes.len() != CASE_ROW_BYTES {
        return Err(error("bad local anchor case row length"));
    }
    decode_case(&bytes).map(Some)
}
fn write_sync(db: &LocalDb, batch: WriteBatch) -> Result<(), MinerErrorV2> {
    let mut options = WriteOptions::default();
    options.set_sync(true);
    options.disable_wal(false);
    db.write_opt(batch, &options).map_err(db_error)
}
fn encode_schema(limits: RocksAnchorDbLimitsV2) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SCHEMA_BYTES);
    bytes.extend_from_slice(SCHEMA_MAGIC);
    for n in [
        limits.max_cases,
        limits.max_sequence,
        limits.max_logical_bytes,
        limits.write_buffer_bytes as u64,
        limits.block_cache_bytes as u64,
    ] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend_from_slice(&limits.max_open_files.to_le_bytes());
    bytes
}
fn decode_schema(bytes: &[u8]) -> Result<RocksAnchorDbLimitsV2, MinerErrorV2> {
    if bytes.len() != SCHEMA_BYTES || &bytes[..8] != SCHEMA_MAGIC {
        return Err(error("unknown local anchor schema"));
    }
    let mut reader = Reader(&bytes[8..]);
    let limits = RocksAnchorDbLimitsV2 {
        max_cases: reader.word()?,
        max_sequence: reader.word()?,
        max_logical_bytes: reader.word()?,
        write_buffer_bytes: usize::try_from(reader.word()?).map_err(|_| MinerErrorV2::Budget)?,
        block_cache_bytes: usize::try_from(reader.word()?).map_err(|_| MinerErrorV2::Budget)?,
        max_open_files: i32::from_le_bytes(reader.take::<4>()?),
    };
    limits.validate()?;
    Ok(limits)
}
fn encode_case(row: CaseRow) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CASE_ROW_BYTES);
    bytes.extend_from_slice(ROW_MAGIC);
    bytes.extend_from_slice(&row.binding.result.0);
    bytes.extend_from_slice(&row.binding.service.0);
    for n in row.binding.process_limits {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    for anchor in [row.current, row.previous] {
        bytes.push(u8::from(anchor.is_some()));
        let anchor = anchor.unwrap_or(JournalAnchorV2 { sequence: 0, hash: Digest([0; 64]) });
        bytes.extend_from_slice(&anchor.sequence.to_le_bytes());
        bytes.extend_from_slice(&anchor.hash.0);
    }
    debug_assert_eq!(bytes.len(), CASE_ROW_BYTES);
    bytes
}
fn decode_case(bytes: &[u8]) -> Result<CaseRow, MinerErrorV2> {
    if bytes.len() != CASE_ROW_BYTES || &bytes[..8] != ROW_MAGIC {
        return Err(error("unknown local anchor row format"));
    }
    let mut r = Reader(&bytes[8..]);
    let result = Digest(r.take::<64>()?);
    let service = Digest(r.take::<64>()?);
    let process_limits = [r.word()?, r.word()?, r.word()?, r.word()?, r.word()?];
    if process_limits.contains(&0) {
        return Err(error("invalid immutable process limit"));
    }
    let current = r.anchor()?;
    let previous = r.anchor()?;
    if current.is_none() && previous.is_some() {
        return Err(error("local anchor has previous without current"));
    }
    if let Some(previous) = previous {
        if current.is_none_or(|next| previous.sequence.checked_add(1) != Some(next.sequence)) {
            return Err(error("noncontiguous local anchor row"));
        }
    } else if current.is_some_and(|current| current.sequence != 0) {
        return Err(error("local anchor initial sequence is not zero"));
    }
    Ok(CaseRow { binding: CaseBinding { result, service, process_limits }, current, previous })
}
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], MinerErrorV2> {
        if self.0.len() < N {
            return Err(error("truncated local anchor value"));
        }
        let (bytes, rest) = self.0.split_at(N);
        self.0 = rest;
        Ok(bytes.try_into().expect("N"))
    }
    fn word(&mut self) -> Result<u64, MinerErrorV2> {
        Ok(u64::from_le_bytes(self.take::<8>()?))
    }
    fn anchor(&mut self) -> Result<Option<JournalAnchorV2>, MinerErrorV2> {
        let tag = self.take::<1>()?[0];
        let sequence = self.word()?;
        let hash = Digest(self.take::<64>()?);
        match tag {
            0 if sequence == 0 && hash == Digest([0; 64]) => Ok(None),
            1 => Ok(Some(JournalAnchorV2 { sequence, hash })),
            _ => Err(error("noncanonical optional local anchor")),
        }
    }
}

fn db_options(limits: RocksAnchorDbLimitsV2, create: bool) -> Options {
    let mut options = Options::default();
    options.create_if_missing(create);
    options.increase_parallelism(1);
    options.set_max_open_files(limits.max_open_files);
    options.set_max_file_opening_threads(1);
    options.set_write_buffer_size(limits.write_buffer_bytes);
    options.set_max_write_buffer_number(2);
    options.set_min_write_buffer_number_to_merge(1);
    options.set_max_total_wal_size(limits.write_buffer_bytes as u64 * 2);
    options.set_target_file_size_base(limits.write_buffer_bytes as u64);
    options.set_max_compaction_bytes(limits.write_buffer_bytes as u64 * 4);
    options.set_max_background_jobs(1);
    options.set_max_subcompactions(1);
    options.set_keep_log_file_num(2);
    options.set_max_log_file_size(limits.write_buffer_bytes);
    options.set_max_manifest_file_size(limits.write_buffer_bytes);
    options.set_delete_obsolete_files_period_micros(60_000_000);
    options.set_bytes_per_sync(1 << 16);
    options.set_wal_bytes_per_sync(1 << 16);
    options.set_use_fsync(true);
    let mut table = BlockBasedOptions::default();
    table.set_block_cache(&Cache::new_lru_cache(limits.block_cache_bytes));
    table.set_cache_index_and_filter_blocks(true);
    options.set_block_based_table_factory(&table);
    options
}
fn require_private_directory(path: &Path) -> Result<(), MinerErrorV2> {
    let metadata = std::fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.file_type().is_dir() || !owned_not_writable_by_others(&metadata) {
        return Err(error("local anchor root is not a real directory"));
    }
    Ok(())
}
fn create_private_directory(path: &Path) -> Result<(), MinerErrorV2> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path).map_err(io_error)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(error("local protected anchor directories require supported ownership semantics"))
    }
}
fn owner_options(create: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    if create {
        options.write(true).create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options
}
fn check_owner_file(path: &Path) -> Result<(), MinerErrorV2> {
    let mut file = owner_options(false).open(path.join(OWNER_FILE)).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file()
        || !owned_not_writable_by_others(&metadata)
        || !single_link(&metadata)
        || metadata.len() != OWNER_BYTES.len() as u64
    {
        return Err(error("unknown dedicated local anchor directory"));
    }
    let mut bytes = vec![0u8; OWNER_BYTES.len()];
    file.read_exact(&mut bytes).map_err(io_error)?;
    if bytes != OWNER_BYTES {
        return Err(error("unknown dedicated local anchor directory"));
    }
    Ok(())
}
fn owned_not_writable_by_others(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o022 == 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}
fn single_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        metadata.nlink() == 1
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}
fn sync_directory(path: &Path) -> Result<(), MinerErrorV2> {
    File::open(path).and_then(|file| file.sync_all()).map_err(io_error)
}
fn sync_parent(path: &Path) -> Result<(), MinerErrorV2> {
    sync_directory(path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))
}
fn error(message: &str) -> MinerErrorV2 {
    MinerErrorV2::Journal(message.into())
}
fn io_error(source: std::io::Error) -> MinerErrorV2 {
    error(&source.to_string())
}
fn db_error(source: ::rocksdb::Error) -> MinerErrorV2 {
    error(source.as_ref())
}

#[cfg(test)]
#[path = "positive_miner_v2_rocksdb_tests.rs"]
mod tests;
