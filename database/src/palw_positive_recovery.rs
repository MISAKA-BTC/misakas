//! Node-local Positive Court recovery transactions. This store knows no verdict,
//! coin rule, or credit flag. Callers supply authenticated/rechecked history and
//! checked native row deltas; archive/head, row deltas and before-images commit
//! in ONE synchronous RocksDB batch. A database read never proves computation.
use crate::prelude::DB;
use parking_lot::Mutex;
use rocksdb::{WriteBatch, WriteOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryHead {
    pub records: u64,
    pub digest: [u8; 64],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryBinding {
    pub branch: [u8; 64],
    pub ruleset: [u8; 64],
    pub result: [u8; 64],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredRecovery {
    pub binding: RecoveryBinding,
    pub head: RecoveryHead,
    pub transaction: [u8; 64],
    pub observed_daa: u64,
    pub archive: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedRow {
    pub key: Vec<u8>,
    pub expected: Option<Vec<u8>>,
    pub value: Option<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryCommit {
    pub case: [u8; 64],
    pub binding: RecoveryBinding,
    pub expected_head: Option<RecoveryHead>,
    pub head: RecoveryHead,
    pub transaction: [u8; 64],
    pub observed_daa: u64,
    pub archive: Vec<u8>,
    pub related: Vec<RelatedRow>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryStoreLimits {
    pub max_archive_bytes: u64,
    pub max_related_rows: u64,
    pub max_key_bytes: u64,
    pub max_value_bytes: u64,
    pub max_transaction_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitEffect {
    Applied,
    AlreadyApplied,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollbackEffect {
    RolledBack,
    AlreadyRolledBack,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PurgeEffect {
    pub deleted_records: u64,
    pub deleted_bytes: u64,
    pub complete: bool,
}
#[derive(Debug, thiserror::Error)]
pub enum RecoveryStoreError {
    #[error("recovery store budget exceeded")]
    Budget,
    #[error("recovery store malformed data")]
    Malformed,
    #[error("recovery store stale head, duplicate transaction, or related row conflict")]
    Conflict,
    #[error("recovery store RocksDB I/O: {0}")]
    Database(#[from] rocksdb::Error),
    #[error("simulated recovery commit acknowledgement failure")]
    SimulatedFailure,
}
pub type RecoveryStoreResult<T> = Result<T, RecoveryStoreError>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum CommitFault {
    BeforeWrite,
    AfterWrite,
}

// Serializes CAS+write for every adapter to the SAME DB (even different namespaces), including
// separately constructed handles. RocksDB itself locks a database directory
// against a second process. Embedding native engines must also hold their
// existing exclusive chain-state writer guard: unrelated direct DB writes do
// not acquire this adapter's lock. Rows owned by this adapter route through it.
type WriterLock = Mutex<()>;
type LockRegistry = BTreeMap<usize, Weak<WriterLock>>;
static LOCKS: OnceLock<Mutex<LockRegistry>> = OnceLock::new();
fn shared_lock(db: &Arc<DB>) -> Arc<WriterLock> {
    let mut registry = LOCKS.get_or_init(|| Mutex::new(BTreeMap::new())).lock();
    registry.retain(|_, v| v.strong_count() != 0);
    let key = Arc::as_ptr(db) as usize;
    if let Some(lock) = registry.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    registry.insert(key, Arc::downgrade(&lock));
    lock
}
#[derive(Clone)]
pub struct PositiveRecoveryStore {
    db: Arc<DB>,
    namespace: Vec<u8>,
    limits: RecoveryStoreLimits,
    lock: Arc<WriterLock>,
}
impl PositiveRecoveryStore {
    /// Namespace belongs to the embedding store; no consensus/native prefix
    /// number is silently assigned here. It must be disjoint from native rows.
    pub fn new(db: Arc<DB>, namespace: Vec<u8>, limits: RecoveryStoreLimits) -> RecoveryStoreResult<Self> {
        if namespace.is_empty()
            || namespace.len() > 32
            || limits.max_archive_bytes == 0
            || limits.max_key_bytes == 0
            || limits.max_transaction_bytes == 0
        {
            return Err(RecoveryStoreError::Budget);
        }
        let lock = shared_lock(&db);
        // Length-delimit the PRIVATE local namespace before any kind/case key.
        // Raw prefixing is ambiguous (e.g. b"a" vs b"a\x01" at kind 1).
        // This encoding assigns no consensus database or protocol wire number.
        let mut encoded_namespace = Vec::with_capacity(namespace.len() + 1);
        encoded_namespace.push(namespace.len() as u8);
        encoded_namespace.extend_from_slice(&namespace);
        Ok(Self { db, namespace: encoded_namespace, limits, lock })
    }
    fn key(&self, kind: u8, case: &[u8; 64], transaction: Option<&[u8; 64]>) -> Vec<u8> {
        let mut key = self.namespace.clone();
        key.push(kind);
        key.extend_from_slice(case);
        if let Some(t) = transaction {
            key.extend_from_slice(t);
        }
        key
    }
    fn bytes(&self, key: &[u8], cap: u64) -> RecoveryStoreResult<Option<Vec<u8>>> {
        let Some(value) = self.db.get_pinned(key)? else { return Ok(None) };
        if value.len() as u64 > cap {
            return Err(RecoveryStoreError::Budget);
        }
        Ok(Some(value.to_vec()))
    }
    fn load_inner(&self, case: &[u8; 64]) -> RecoveryStoreResult<Option<StoredRecovery>> {
        self.bytes(&self.key(0, case, None), self.limits.max_transaction_bytes)?.map(|b| decode_state(&b, self.limits)).transpose()
    }
    pub fn load(&self, case: &[u8; 64]) -> RecoveryStoreResult<Option<StoredRecovery>> {
        let _lock = self.lock.lock();
        self.load_inner(case)
    }
    /// A raw native row read for embedding adapters and restart verification.
    /// No cache is populated before a successful transaction write.
    pub fn related_value(&self, key: &[u8]) -> RecoveryStoreResult<Option<Vec<u8>>> {
        let _lock = self.lock.lock();
        self.validate_key(key)?;
        self.bytes(key, self.limits.max_value_bytes)
    }
    fn validate_key(&self, key: &[u8]) -> RecoveryStoreResult<()> {
        if key.is_empty() || key.len() as u64 > self.limits.max_key_bytes {
            return Err(RecoveryStoreError::Budget);
        }
        if key.starts_with(&self.namespace) {
            return Err(RecoveryStoreError::Conflict);
        }
        Ok(())
    }
    fn validate_rows(&self, rows: &[RelatedRow]) -> RecoveryStoreResult<()> {
        if rows.len() as u64 > self.limits.max_related_rows {
            return Err(RecoveryStoreError::Budget);
        }
        let mut unique = BTreeSet::new();
        for row in rows {
            self.validate_key(&row.key)?;
            if !unique.insert(&row.key) {
                return Err(RecoveryStoreError::Conflict);
            }
            if [&row.expected, &row.value].into_iter().flatten().any(|v| v.len() as u64 > self.limits.max_value_bytes) {
                return Err(RecoveryStoreError::Budget);
            }
        }
        Ok(())
    }
    pub fn commit(&self, commit: RecoveryCommit) -> RecoveryStoreResult<CommitEffect> {
        self.commit_inner(commit, None)
    }
    #[doc(hidden)]
    pub fn commit_with_fault(&self, commit: RecoveryCommit, fault: CommitFault) -> RecoveryStoreResult<CommitEffect> {
        self.commit_inner(commit, Some(fault))
    }
    fn commit_inner(&self, commit: RecoveryCommit, fault: Option<CommitFault>) -> RecoveryStoreResult<CommitEffect> {
        let _lock = self.lock.lock();
        self.validate_rows(&commit.related)?;
        if commit.archive.len() as u64 > self.limits.max_archive_bytes {
            return Err(RecoveryStoreError::Budget);
        }
        let next = StoredRecovery {
            binding: commit.binding,
            head: commit.head,
            transaction: commit.transaction,
            observed_daa: commit.observed_daa,
            archive: commit.archive,
        };
        let encoded = encode_state(&next)?;
        if encoded.len() as u64 > self.limits.max_transaction_bytes {
            return Err(RecoveryStoreError::Budget);
        }
        let current = self.load_inner(&commit.case)?;
        let undo_key = self.key(1, &commit.case, Some(&commit.transaction));
        let rolled_key = self.key(2, &commit.case, Some(&commit.transaction));
        if let Some(receipt) = self.bytes(&rolled_key, self.limits.max_transaction_bytes)? {
            let (rolled, rows) = decode_undo(&receipt, self.limits)?;
            if rolled.as_ref() != Some(&next) || rows != commit.related {
                return Err(RecoveryStoreError::Conflict);
            }
        }
        if current.as_ref().is_some_and(|c| c.transaction == commit.transaction) {
            if current.as_ref() != Some(&next) {
                return Err(RecoveryStoreError::Conflict);
            }
            let undo = self.bytes(&undo_key, self.limits.max_transaction_bytes)?.ok_or(RecoveryStoreError::Malformed)?;
            let (_, saved_rows) = decode_undo(&undo, self.limits)?;
            if saved_rows != commit.related {
                return Err(RecoveryStoreError::Conflict);
            }
            for row in &commit.related {
                if self.bytes(&row.key, self.limits.max_value_bytes)? != row.value {
                    return Err(RecoveryStoreError::Conflict);
                }
            }
            return Ok(CommitEffect::AlreadyApplied);
        }
        if current.as_ref().map(|c| c.head) != commit.expected_head
            || current.as_ref().is_some_and(|c| {
                c.binding != next.binding
                    || next.observed_daa < c.observed_daa
                    || !(next.head.records > c.head.records || (next.head == c.head && next.observed_daa > c.observed_daa))
            })
            || self.db.get_pinned(&undo_key)?.is_some()
        {
            return Err(RecoveryStoreError::Conflict);
        }
        for row in &commit.related {
            if self.bytes(&row.key, self.limits.max_value_bytes)? != row.expected {
                return Err(RecoveryStoreError::Conflict);
            }
        }
        let undo = encode_undo(current.as_ref(), &commit.related)?;
        let total = (encoded.len() as u64).checked_add(undo.len() as u64).ok_or(RecoveryStoreError::Budget)?;
        if total > self.limits.max_transaction_bytes {
            return Err(RecoveryStoreError::Budget);
        }
        let mut batch = WriteBatch::default();
        batch.put(self.key(0, &commit.case, None), encoded);
        batch.put(undo_key, undo);
        batch.delete(rolled_key);
        for row in &commit.related {
            put_or_delete(&mut batch, &row.key, row.value.as_deref());
        }
        if batch.size_in_bytes() as u64 > self.limits.max_transaction_bytes {
            return Err(RecoveryStoreError::Budget);
        }
        if fault == Some(CommitFault::BeforeWrite) {
            return Err(RecoveryStoreError::SimulatedFailure);
        }
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db.write_opt(batch, &options)?;
        if fault == Some(CommitFault::AfterWrite) {
            return Err(RecoveryStoreError::SimulatedFailure);
        }
        Ok(CommitEffect::Applied)
    }
    /// Bounded retirement of before-images AFTER the embedding adapter has
    /// verified every retention horizon and a full-history replacement token.
    /// This function cannot infer that authority from a stored pass flag.
    /// Archive, current head/clock and related rows are left untouched. A
    /// retired transaction can no longer be rolled back or reused as authority.
    pub fn purge_before_images(
        &self,
        case: &[u8; 64],
        expected_head: RecoveryHead,
        expected_observed_daa: u64,
        max_rows: u64,
        max_bytes: u64,
    ) -> RecoveryStoreResult<PurgeEffect> {
        let _lock = self.lock.lock();
        if max_rows == 0 || max_bytes == 0 {
            return Err(RecoveryStoreError::Budget);
        }
        let current = self.load_inner(case)?.ok_or(RecoveryStoreError::Conflict)?;
        if current.head != expected_head || current.observed_daa != expected_observed_daa {
            return Err(RecoveryStoreError::Conflict);
        }
        let cap = max_bytes.min(self.limits.max_transaction_bytes);
        let mut keys = Vec::new();
        let mut deleted_bytes = 0u64;
        let mut complete = true;
        'prefixes: for kind in [1, 2] {
            let prefix = self.key(kind, case, None);
            let mut iter = self.db.raw_iterator();
            iter.seek(&prefix);
            while iter.valid() {
                let key = iter.key().ok_or(RecoveryStoreError::Malformed)?;
                if !key.starts_with(&prefix) {
                    break;
                }
                if key.len() != prefix.len() + 64 {
                    return Err(RecoveryStoreError::Malformed);
                }
                let value = iter.value().ok_or(RecoveryStoreError::Malformed)?;
                let bytes = (key.len() as u64).checked_add(value.len() as u64).ok_or(RecoveryStoreError::Budget)?;
                let total = deleted_bytes.checked_add(bytes).ok_or(RecoveryStoreError::Budget)?;
                if keys.len() as u64 >= max_rows || total > cap {
                    complete = false;
                    break 'prefixes;
                }
                // raw_iterator borrows the value, so even a corrupt huge value
                // is size-checked before any owned buffer is allocated.
                keys.push(key.to_vec());
                deleted_bytes = total;
                iter.next();
            }
            iter.status()?;
        }
        if keys.is_empty() && !complete {
            return Err(RecoveryStoreError::Budget);
        }
        if !keys.is_empty() {
            let mut batch = WriteBatch::default();
            for key in &keys {
                batch.delete(key);
            }
            if batch.size_in_bytes() as u64 > self.limits.max_transaction_bytes {
                return Err(RecoveryStoreError::Budget);
            }
            let mut options = WriteOptions::default();
            options.set_sync(true);
            self.db.write_opt(batch, &options)?;
        }
        Ok(PurgeEffect { deleted_records: keys.len() as u64, deleted_bytes, complete })
    }

    /// Restore all before-images in one synchronous batch. Refuses to undo a
    /// stale transaction or overwrite native rows changed by another case.
    pub fn rollback(&self, case: &[u8; 64], transaction: &[u8; 64]) -> RecoveryStoreResult<RollbackEffect> {
        let _lock = self.lock.lock();
        let rolled_key = self.key(2, case, Some(transaction));
        if let Some(receipt) = self.bytes(&rolled_key, self.limits.max_transaction_bytes)? {
            let (rolled, rows) = decode_undo(&receipt, self.limits)?;
            self.validate_rows(&rows)?;
            if rolled.as_ref().is_none_or(|r| &r.transaction != transaction) {
                return Err(RecoveryStoreError::Malformed);
            }
            return Ok(RollbackEffect::AlreadyRolledBack);
        }
        let current = self.load_inner(case)?.ok_or(RecoveryStoreError::Conflict)?;
        if &current.transaction != transaction {
            return Err(RecoveryStoreError::Conflict);
        }
        let undo_key = self.key(1, case, Some(transaction));
        let undo = self.bytes(&undo_key, self.limits.max_transaction_bytes)?.ok_or(RecoveryStoreError::Malformed)?;
        let (previous, rows) = decode_undo(&undo, self.limits)?;
        self.validate_rows(&rows)?;
        for row in &rows {
            if self.bytes(&row.key, self.limits.max_value_bytes)? != row.value {
                return Err(RecoveryStoreError::Conflict);
            }
        }
        let mut batch = WriteBatch::default();
        let previous = previous.as_ref().map(encode_state).transpose()?;
        put_or_delete(&mut batch, &self.key(0, case, None), previous.as_deref());
        for row in &rows {
            put_or_delete(&mut batch, &row.key, row.expected.as_deref());
        }
        batch.delete(undo_key);
        // Keep the exact rolled transaction so A -> B -> A may reapply it,
        // while reuse of that transaction ID for different contents is denied.
        batch.put(rolled_key, encode_undo(Some(&current), &rows)?);
        if batch.size_in_bytes() as u64 > self.limits.max_transaction_bytes {
            return Err(RecoveryStoreError::Budget);
        }
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db.write_opt(batch, &options)?;
        Ok(RollbackEffect::RolledBack)
    }
}
fn put_or_delete(batch: &mut WriteBatch, key: &[u8], value: Option<&[u8]>) {
    match value {
        Some(v) => batch.put(key, v),
        None => batch.delete(key),
    }
}
fn blob(out: &mut Vec<u8>, bytes: &[u8]) -> RecoveryStoreResult<()> {
    out.extend_from_slice(&u64::try_from(bytes.len()).map_err(|_| RecoveryStoreError::Budget)?.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn optional(out: &mut Vec<u8>, value: Option<&[u8]>) -> RecoveryStoreResult<()> {
    out.push(u8::from(value.is_some()));
    if let Some(value) = value {
        blob(out, value)?;
    }
    Ok(())
}
fn encode_state(state: &StoredRecovery) -> RecoveryStoreResult<Vec<u8>> {
    let mut b = Vec::new();
    b.extend_from_slice(b"PCDB1");
    for h in [&state.binding.branch, &state.binding.ruleset, &state.binding.result, &state.head.digest, &state.transaction] {
        b.extend_from_slice(h);
    }
    b.extend_from_slice(&state.head.records.to_le_bytes());
    b.extend_from_slice(&state.observed_daa.to_le_bytes());
    blob(&mut b, &state.archive)?;
    Ok(b)
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> RecoveryStoreResult<&'a [u8]> {
        let (s, rest) = self.0.split_at_checked(n).ok_or(RecoveryStoreError::Malformed)?;
        self.0 = rest;
        Ok(s)
    }
    fn word(&mut self) -> RecoveryStoreResult<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn hash(&mut self) -> RecoveryStoreResult<[u8; 64]> {
        Ok(self.take(64)?.try_into().unwrap())
    }
    fn blob(&mut self, cap: u64) -> RecoveryStoreResult<Vec<u8>> {
        let n = self.word()?;
        if n > cap {
            return Err(RecoveryStoreError::Budget);
        }
        let n = usize::try_from(n).map_err(|_| RecoveryStoreError::Budget)?;
        Ok(self.take(n)?.to_vec())
    }
    fn optional(&mut self, cap: u64) -> RecoveryStoreResult<Option<Vec<u8>>> {
        match self.take(1)?[0] {
            0 => Ok(None),
            1 => self.blob(cap).map(Some),
            _ => Err(RecoveryStoreError::Malformed),
        }
    }
    fn end(self) -> RecoveryStoreResult<()> {
        if self.0.is_empty() { Ok(()) } else { Err(RecoveryStoreError::Malformed) }
    }
}
fn decode_state(bytes: &[u8], limits: RecoveryStoreLimits) -> RecoveryStoreResult<StoredRecovery> {
    let mut r = Reader(bytes);
    if r.take(5)? != b"PCDB1" {
        return Err(RecoveryStoreError::Malformed);
    }
    let binding = RecoveryBinding { branch: r.hash()?, ruleset: r.hash()?, result: r.hash()? };
    let digest = r.hash()?;
    let transaction = r.hash()?;
    let records = r.word()?;
    let observed_daa = r.word()?;
    let archive = r.blob(limits.max_archive_bytes)?;
    r.end()?;
    Ok(StoredRecovery { binding, head: RecoveryHead { records, digest }, transaction, observed_daa, archive })
}
fn encode_undo(previous: Option<&StoredRecovery>, rows: &[RelatedRow]) -> RecoveryStoreResult<Vec<u8>> {
    let mut b = b"PCUNDO1".to_vec();
    let previous = previous.map(encode_state).transpose()?;
    optional(&mut b, previous.as_deref())?;
    b.extend_from_slice(&(rows.len() as u64).to_le_bytes());
    for row in rows {
        blob(&mut b, &row.key)?;
        optional(&mut b, row.expected.as_deref())?;
        optional(&mut b, row.value.as_deref())?;
    }
    Ok(b)
}
fn decode_undo(bytes: &[u8], limits: RecoveryStoreLimits) -> RecoveryStoreResult<(Option<StoredRecovery>, Vec<RelatedRow>)> {
    let mut r = Reader(bytes);
    if r.take(7)? != b"PCUNDO1" {
        return Err(RecoveryStoreError::Malformed);
    }
    let previous = r.optional(limits.max_transaction_bytes)?.map(|b| decode_state(&b, limits)).transpose()?;
    let count = r.word()?;
    if count > limits.max_related_rows {
        return Err(RecoveryStoreError::Budget);
    }
    let mut rows = Vec::new();
    for _ in 0..count {
        rows.push(RelatedRow {
            key: r.blob(limits.max_key_bytes)?,
            expected: r.optional(limits.max_value_bytes)?,
            value: r.optional(limits.max_value_bytes)?,
        });
    }
    r.end()?;
    Ok((previous, rows))
}
#[cfg(test)]
#[path = "palw_positive_recovery_tests.rs"]
mod tests;
