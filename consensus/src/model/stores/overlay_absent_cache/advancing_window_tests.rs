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
        assert_eq!(cache.entries.lock().rows.len(), capacity);
    }
    let entries = cache.entries.lock();
    let expected: Vec<_> = (steps..capacity + steps).collect();
    assert_eq!(
        entries.rows.keys().copied().collect::<Vec<_>>(),
        expected,
        "all stale prefix rows leave; insertion order remains FIFO without extra key storage"
    );
}

#[test]
fn advancing_sparse_overlay_window_stays_cached_after_multiple_capacity_turnovers() {
    advancing_window(16, 8, 3 * 16);
}

#[test]
fn production_absence_limit_retains_recent_window_without_growing_storage() {
    advancing_window(MAX_ABSENT_OVERLAY_ROWS, 32, 64);
}
