//! Advancing sparse-overlay reads through the actual cache, with a counted
//! missing-row backend. No source-string assertions or timing claims.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn counted_absence(cache: &OverlayAbsentCache<usize>, key: usize, reads: &AtomicUsize) {
    assert!(matches!(
        cache.read(key, || {
            reads.fetch_add(1, Ordering::Relaxed);
            Err::<u64, _>(StoreError::KeyNotFound(DbKey::prefix_only(b"advancing-overlay")))
        }),
        Err(StoreError::KeyNotFound(_))
    ));
}

fn advancing_window(capacity: usize, window: usize, steps: usize) {
    assert!(window < capacity && steps > window);
    let cache = OverlayAbsentCache::new(capacity);
    let reads = AtomicUsize::new(0);
    for key in 0..capacity {
        counted_absence(&cache, key, &reads);
    }
    assert_eq!(reads.load(Ordering::Relaxed), capacity);
    for head in capacity..capacity + steps {
        let before = reads.load(Ordering::Relaxed);
        // Same newest-to-oldest walk as selected_chain_overlay_window. Only
        // the newly accepted head is absent from this bounded recent window.
        for key in (head + 1 - window..=head).rev() {
            counted_absence(&cache, key, &reads);
        }
        assert_eq!(
            reads.load(Ordering::Relaxed) - before,
            1,
            "head {head}: an advancing cached window must not reread older missing rows"
        );
        let retained = cache.entries.lock().rows.len();
        assert!(retained > capacity / 2 && retained <= capacity);
    }
    let entries = cache.entries.lock();
    let retained = entries.rows.len();
    let expected: Vec<_> = (capacity + steps - retained..capacity + steps).collect();
    assert_eq!(
        entries.rows.keys().copied().collect::<Vec<_>>(),
        expected,
        "oldest prefixes leave in batches; the retained insertion order stays FIFO without extra key storage"
    );
    assert!(entries.eviction_scanned_rows <= 2 * (steps + capacity));
}

#[test]
fn advancing_sparse_overlay_window_stays_cached_after_multiple_capacity_turnovers() {
    advancing_window(16, 8, 3 * 16);
}

#[test]
fn production_absence_limit_retains_recent_window_without_growing_storage() {
    advancing_window(MAX_ABSENT_OVERLAY_ROWS, 32, 64);
}

#[test]
fn full_capacity_eviction_work_is_amortized_across_multiple_turnovers() {
    let capacity = MAX_ABSENT_OVERLAY_ROWS;
    let misses = 2 * capacity;
    let cache = OverlayAbsentCache::new(capacity);
    let reads = AtomicUsize::new(0);
    for key in 0..capacity + misses {
        counted_absence(&cache, key, &reads);
    }
    assert_eq!(reads.load(Ordering::Relaxed), capacity + misses);
    let entries = cache.entries.lock();
    assert_eq!(entries.eviction_batches, 4);
    assert_eq!(entries.eviction_scanned_rows, 4 * capacity);
    // The old one-row shift visits a full table on every miss. Count work,
    // not wall time: allocator, hardware and profile must not decide the test.
    let old_scanned_rows = misses * capacity;
    assert!(entries.eviction_scanned_rows * 1_000 < old_scanned_rows);
    assert_eq!(entries.rows.len(), capacity);
    assert_eq!(entries.rows.keys().next(), Some(&misses));
    assert_eq!(entries.rows.keys().last(), Some(&(capacity + misses - 1)));
}

#[test]
fn batch_eviction_retries_old_absence_and_preserves_current_write_invalidation() {
    let cache = OverlayAbsentCache::new(8);
    let reads = AtomicUsize::new(0);
    for key in 0..9 {
        counted_absence(&cache, key, &reads);
    }
    assert_eq!(cache.entries.lock().rows.keys().copied().collect::<Vec<_>>(), (4..9).collect::<Vec<_>>());
    // Evicted metadata makes a fresh read, including a row that now exists.
    assert_eq!(cache.read(0, || Ok(42)).unwrap(), 42);
    // A pending batch still cannot publish an absence or borrow one from a
    // sibling, before or after an eviction batch.
    let (_, ticket) = cache.mutate_batch_tracked(&8, || Ok(())).unwrap();
    counted_absence(&cache, 8, &reads);
    assert!(!cache.entries.lock().rows.contains_key(&8));
    ticket.committed();
    assert_eq!(cache.read(8, || Ok(99)).unwrap(), 99);
}
