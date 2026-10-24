//! Bounded caching of absent overlay rows, without changing generic DB caches.
//!
//! Sparse overlay stores deliberately omit empty rows. A repeated missing-key
//! RocksDB lookup may nevertheless read and checksum a large adjacent SST data
//! block. Only `KeyNotFound` is reusable; I/O and decoding failures are not.
//! All mutations of the owning store must pass through `mutate`, including
//! batched writes. The shared lock prevents a late reader from publishing an
//! obsolete absence after an insert, delete, or restored row.

use indexmap::IndexMap;
use kaspa_database::prelude::{CachePolicy, DbKey, StoreError};
use parking_lot::Mutex;
use std::{hash::Hash, sync::Arc};

const MAX_ABSENT_OVERLAY_ROWS: usize = 16_384;

#[derive(Clone)]
pub(super) struct OverlayAbsentCache<K> {
    entries: Arc<Mutex<AbsentEntries<K>>>,
    invalidation: Arc<Mutex<Invalidation<K>>>,
    limit: usize,
}

struct Invalidation<K> {
    generation: u64,
    pending: IndexMap<K, usize>,
    disabled: bool,
}

struct AbsentEntries<K> {
    generation: u64,
    rows: IndexMap<K, DbKey>,
}

/// Acknowledged only after the corresponding RocksDB batch committed successfully.
/// Dropping a ticket deliberately leaves the key pending: enqueue/abandon does
/// not prove durability. Tickets cannot be cloned or acknowledged twice.
#[must_use = "acknowledge after successful DB commit; dropping keeps safe uncached fallback"]
pub(crate) struct OverlayCommitTicket<K> {
    key: K,
    entries: Arc<Mutex<AbsentEntries<K>>>,
    invalidation: Arc<Mutex<Invalidation<K>>>,
}

impl<K: Eq + Hash> OverlayCommitTicket<K> {
    pub(crate) fn committed(self) {
        let mut state = self.invalidation.lock();
        let previous = state.generation;
        state.generation = previous.checked_add(1).expect("overlay cache mutation generation exhausted");
        if !state.disabled {
            let count = state.pending.get_mut(&self.key).expect("acknowledged overlay key must be pending");
            if *count > 1 {
                *count -= 1;
            } else {
                state.pending.swap_remove(&self.key);
            }
        }
        let mut entries = self.entries.lock();
        // Retain this cache's unrelated stable misses across each block commit.
        // An independent sibling write still invalidates all its older misses.
        if entries.generation != previous || state.disabled {
            entries.rows.clear();
        }
        entries.generation = state.generation;
        entries.rows.swap_remove(&self.key);
    }
}

impl<K: Clone + Eq + Hash> OverlayAbsentCache<K> {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(AbsentEntries { generation: 0, rows: IndexMap::new() })),
            invalidation: Arc::new(Mutex::new(Invalidation { generation: 0, pending: IndexMap::new(), disabled: false })),
            limit: limit.min(MAX_ABSENT_OVERLAY_ROWS),
        }
    }

    /// Independent empty cache, sharing the write/restore invalidation domain.
    pub(super) fn clone_with_new_cache(&self, limit: usize) -> Self {
        let mut cloned = Self::new(limit);
        cloned.invalidation = self.invalidation.clone();
        cloned
    }

    pub(super) fn clone_for_policy(&self, policy: CachePolicy) -> Self {
        self.clone_with_new_cache(Self::for_policy(policy).limit)
    }

    pub(super) fn for_policy(policy: CachePolicy) -> Self {
        // Do not add hidden rows to a disabled cache or convert a byte budget
        // into an untracked count budget. These overlay stores use Count today.
        Self::new(match policy {
            CachePolicy::Count(count) => count,
            CachePolicy::Empty | CachePolicy::Tracked { .. } => 0,
        })
    }

    pub(super) fn read<T>(&self, key: K, read: impl FnOnce() -> Result<T, StoreError>) -> Result<T, StoreError> {
        if self.limit == 0 {
            return read();
        }
        // A fresh cache also shares this lock/generation with every writer.
        // Therefore sibling writes cannot leave an older cached miss visible.
        let state = self.invalidation.lock();
        let mut entries = self.entries.lock();
        if entries.generation != state.generation {
            entries.rows.clear();
            entries.generation = state.generation;
        }
        // A queued batch is not yet durable. Independent positive caches can
        // read the old DB state until its commit. Never remember that absence.
        if state.disabled || state.pending.contains_key(&key) {
            return read();
        }
        if let Some(db_key) = entries.rows.get(&key) {
            return Err(StoreError::KeyNotFound(db_key.clone()));
        }
        let result = read();
        if let Err(StoreError::KeyNotFound(db_key)) = &result {
            if entries.rows.len() == self.limit {
                // Bounded FIFO metadata movement, without another key queue.
                // swap_remove would move the newest key into slot zero and
                // repeatedly evict it while stale middle entries never leave.
                entries.rows.shift_remove_index(0);
            }
            entries.rows.insert(key, db_key.clone());
        }
        result
    }

    pub(super) fn mutate<T>(&self, key: &K, mutation: impl FnOnce() -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.mutate_inner(key, false, mutation)
    }

    /// Legacy unacknowledged enqueue: conservatively keep the key pending.
    pub(super) fn mutate_batch<T>(&self, key: &K, mutation: impl FnOnce() -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.mutate_inner(key, true, mutation)
    }

    pub(super) fn mutate_batch_tracked<T>(
        &self,
        key: &K,
        mutation: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<(T, OverlayCommitTicket<K>), StoreError> {
        let value = self.mutate_inner(key, true, mutation)?;
        Ok((value, OverlayCommitTicket { key: key.clone(), entries: self.entries.clone(), invalidation: self.invalidation.clone() }))
    }

    fn mutate_inner<T>(&self, key: &K, batch: bool, mutation: impl FnOnce() -> Result<T, StoreError>) -> Result<T, StoreError> {
        // Even a disabled sibling can write rows observed by a cached sibling.
        let mut state = self.invalidation.lock();
        let previous = state.generation;
        state.generation = previous.checked_add(1).expect("overlay cache mutation generation exhausted");
        if batch && !state.disabled {
            if let Some(count) = state.pending.get_mut(key) {
                if *count == MAX_ABSENT_OVERLAY_ROWS {
                    state.pending.clear();
                    state.disabled = true;
                } else {
                    *count += 1;
                }
            } else if state.pending.len() == MAX_ABSENT_OVERLAY_ROWS {
                // Permanently fall back to ordinary DB reads for this store
                // instance. No unbounded dirty-key list or unsafe eviction.
                state.pending.clear();
                state.disabled = true;
            } else {
                state.pending.insert(key.clone(), 1);
            }
        }
        let mut entries = self.entries.lock();
        if entries.generation != previous || state.disabled {
            entries.rows.clear();
        }
        entries.generation = state.generation;
        entries.rows.swap_remove(key);
        // Hold the shared lock through cache/DB mutation: no late miss publish.
        // A failed/abandoned batch keeps its pending mark conservatively.
        mutation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };

    fn missing<T>() -> Result<T, StoreError> {
        Err(StoreError::KeyNotFound(DbKey::prefix_only(b"overlay-test")))
    }

    #[test]
    fn repeated_absence_avoids_reads_but_errors_are_retried() {
        let cache = OverlayAbsentCache::new(2);
        let reads = AtomicUsize::new(0);
        for _ in 0..8 {
            assert!(
                cache
                    .read(1, || {
                        reads.fetch_add(1, Ordering::Relaxed);
                        missing::<u64>()
                    })
                    .is_err()
            );
        }
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        for _ in 0..2 {
            assert!(matches!(
                cache.read(2, || {
                    reads.fetch_add(1, Ordering::Relaxed);
                    Err::<u64, _>(StoreError::DataInconsistency("corrupt".into()))
                }),
                Err(StoreError::DataInconsistency(_))
            ));
        }
        assert_eq!(reads.load(Ordering::Relaxed), 3);
        assert_eq!(cache.entries.lock().rows.len(), 1);
    }

    #[test]
    fn bounded_disabled_and_failed_mutation_invalidation() {
        let cache = OverlayAbsentCache::new(2);
        for key in 0..100 {
            let _ = cache.read(key, missing::<u64>);
        }
        assert_eq!(cache.entries.lock().rows.len(), 2);
        assert!(cache.entries.lock().rows.contains_key(&99));
        assert!(cache.mutate(&99, || Err::<(), _>(StoreError::DataInconsistency("write failed".into()))).is_err());
        assert!(!cache.entries.lock().rows.contains_key(&99));
        assert_eq!(cache.read(99, || Ok(7)).unwrap(), 7);
        assert_eq!(OverlayAbsentCache::<u64>::new(usize::MAX).limit, MAX_ABSENT_OVERLAY_ROWS);
        for policy in [CachePolicy::Empty, CachePolicy::Count(0)] {
            let disabled = OverlayAbsentCache::for_policy(policy);
            let _ = disabled.read(1, missing::<u64>);
            assert!(disabled.entries.lock().rows.is_empty());
        }
    }

    #[test]
    fn fresh_and_disabled_sibling_writes_invalidate_old_absence() {
        let cache = OverlayAbsentCache::new(2);
        let _ = cache.read(1, missing::<u64>);
        let fresh = cache.clone_with_new_cache(2);
        assert!(fresh.entries.lock().rows.is_empty());
        fresh.mutate(&1, || Ok(())).unwrap();
        assert_eq!(cache.read(1, || Ok(42)).unwrap(), 42);
        let _ = cache.read(2, missing::<u64>);
        let disabled = cache.clone_for_policy(CachePolicy::Empty);
        disabled.mutate(&2, || Ok(())).unwrap();
        assert_eq!(cache.read(2, || Ok(99)).unwrap(), 99);
    }

    #[test]
    fn queued_batch_misses_are_never_published_in_new_sibling_cache() {
        let cache = OverlayAbsentCache::new(2);
        let _ = cache.read(1, missing::<u64>);
        cache.mutate_batch(&1, || Ok(())).unwrap();
        let sibling = cache.clone_with_new_cache(2);
        // The DB still contains no row after enqueueing an insert.
        assert!(sibling.read(1, missing::<u64>).is_err());
        assert!(cache.read(1, missing::<u64>).is_err());
        assert!(sibling.entries.lock().rows.is_empty());
        assert!(cache.entries.lock().rows.is_empty());
        // Commit happens without another store method/generation advance.
        assert_eq!(sibling.read(1, || Ok(42)).unwrap(), 42);
        assert_eq!(cache.read(1, || Ok(42)).unwrap(), 42);
        // Dropped/failed batches remain conservative; unrelated missing rows
        // can still be cached before the pending-key bound is reached.
        let _ = cache.read(2, missing::<u64>);
        assert!(cache.entries.lock().rows.contains_key(&2));
    }

    #[test]
    fn pending_saturation_disables_without_an_unbounded_dirty_list() {
        let cache = OverlayAbsentCache::new(2);
        let _ = cache.read(usize::MAX, missing::<u64>);
        for key in 0..MAX_ABSENT_OVERLAY_ROWS {
            cache.mutate_batch(&key, || Ok(())).unwrap();
        }
        assert_eq!(cache.invalidation.lock().pending.len(), MAX_ABSENT_OVERLAY_ROWS);
        cache.mutate_batch(&MAX_ABSENT_OVERLAY_ROWS, || Ok(())).unwrap();
        assert!(cache.invalidation.lock().disabled);
        assert!(cache.invalidation.lock().pending.is_empty());
        assert!(cache.entries.lock().rows.is_empty());
        let sibling = cache.clone_with_new_cache(2);
        for _ in 0..2 {
            let _ = sibling.read(usize::MAX, missing::<u64>);
        }
        assert!(sibling.entries.lock().rows.is_empty());
    }

    #[test]
    fn acknowledged_batches_retire_pending_without_flushing_other_misses() {
        let cache = OverlayAbsentCache::new(2);
        let reads = AtomicUsize::new(0);
        let _ = cache.read(usize::MAX, || {
            reads.fetch_add(1, Ordering::Relaxed);
            missing::<u64>()
        });
        for key in 0..MAX_ABSENT_OVERLAY_ROWS * 2 {
            let (_, ticket) = cache.mutate_batch_tracked(&key, || Ok(())).unwrap();
            assert_eq!(cache.invalidation.lock().pending.get(&key), Some(&1));
            ticket.committed();
            assert!(cache.invalidation.lock().pending.is_empty());
            let _ = cache.read(usize::MAX, || {
                reads.fetch_add(1, Ordering::Relaxed);
                missing::<u64>()
            });
        }
        assert!(!cache.invalidation.lock().disabled);
        assert_eq!(reads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn same_key_reverse_commit_keeps_pending_until_every_ticket_acknowledged() {
        let cache = OverlayAbsentCache::new(2);
        let (_, insert) = cache.mutate_batch_tracked(&1, || Ok(())).unwrap();
        let (_, delete) = cache.mutate_batch_tracked(&1, || Ok(())).unwrap();
        assert_eq!(cache.invalidation.lock().pending.get(&1), Some(&2));
        // The newer delete commits first, while the older insert is still queued.
        delete.committed();
        assert_eq!(cache.invalidation.lock().pending.get(&1), Some(&1));
        let sibling = cache.clone_with_new_cache(2);
        assert!(sibling.read(1, missing::<u64>).is_err());
        assert!(sibling.entries.lock().rows.is_empty());
        insert.committed();
        assert!(cache.invalidation.lock().pending.is_empty());
        assert_eq!(sibling.read(1, || Ok(42)).unwrap(), 42);
    }

    #[test]
    fn abandoned_failed_and_unwound_tickets_cannot_release_pending_absence() {
        let cache = OverlayAbsentCache::new(2);
        let (_, abandoned) = cache.mutate_batch_tracked(&1, || Ok(())).unwrap();
        drop(abandoned);
        let (_, successful) = cache.mutate_batch_tracked(&1, || Ok(())).unwrap();
        successful.committed();
        assert_eq!(cache.invalidation.lock().pending.get(&1), Some(&1));
        assert!(cache.mutate_batch_tracked(&2, || Err::<(), _>(StoreError::DataInconsistency("enqueue failure".into()))).is_err());
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (_, _ticket) = cache.mutate_batch_tracked(&3, || Ok(())).unwrap();
            panic!("abandon staged batch");
        }));
        assert!(unwound.is_err());
        for key in 1..=3 {
            let _ = cache.read(key, missing::<u64>);
            assert!(!cache.entries.lock().rows.contains_key(&key));
            assert_eq!(cache.invalidation.lock().pending.get(&key), Some(&1));
        }
    }

    #[test]
    fn same_key_abandoned_ticket_bound_disables_safely() {
        let cache = OverlayAbsentCache::new(2);
        for _ in 0..=MAX_ABSENT_OVERLAY_ROWS {
            let (_, ticket) = cache.mutate_batch_tracked(&1, || Ok(())).unwrap();
            drop(ticket);
        }
        assert!(cache.invalidation.lock().disabled);
        assert!(cache.invalidation.lock().pending.is_empty());
        let (_, ticket) = cache.mutate_batch_tracked(&1, || Ok(())).unwrap();
        ticket.committed();
        assert!(cache.invalidation.lock().disabled);
        let _ = cache.read(1, missing::<u64>);
        assert!(cache.entries.lock().rows.is_empty());
    }

    #[test]
    fn late_miss_cannot_hide_concurrent_insert_or_restore() {
        let cache = OverlayAbsentCache::new(2);
        let value = Arc::new(AtomicUsize::new(0));
        let (reading, entered) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let reader_cache = cache.clone();
        let reader = std::thread::spawn(move || {
            reader_cache.read(1, || {
                reading.send(()).unwrap();
                resumed.recv().unwrap();
                missing::<usize>()
            })
        });
        entered.recv().unwrap();
        let writer_cache = cache.clone();
        let writer_value = value.clone();
        let writer = std::thread::spawn(move || {
            writer_cache.mutate(&1, || {
                writer_value.store(42, Ordering::SeqCst);
                Ok(())
            })
        });
        resume.send(()).unwrap();
        assert!(reader.join().unwrap().is_err());
        writer.join().unwrap().unwrap();
        assert_eq!(cache.read(1, || Ok(value.load(Ordering::SeqCst))).unwrap(), 42);
        // Delete, then restore the same hash: both must invalidate absence.
        cache
            .mutate(&1, || {
                value.store(0, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        let _ = cache.read(1, missing::<usize>);
        cache
            .mutate(&1, || {
                value.store(99, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        assert_eq!(cache.read(1, || Ok(value.load(Ordering::SeqCst))).unwrap(), 99);
    }
}

#[cfg(test)]
#[path = "overlay_absent_cache/advancing_window_tests.rs"]
mod advancing_window_tests;
