//! Per-thread requested allocation bytes for these source controls only. This
//! counts allocator requests, not live heap/RSS, whole-node peaks or quotas.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static REQUESTED: Cell<usize> = const { Cell::new(0) };
}
struct CountingAllocator;
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn account(bytes: usize) {
    let _ = TRACK.try_with(|track| {
        if track.get() {
            let _ = REQUESTED.try_with(|total| total.set(total.get().saturating_add(bytes)));
        }
    });
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        account(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        account(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        account(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

pub(super) fn requested<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    TRACK.with(|flag| {
        assert!(!flag.get(), "nested allocator observation");
        flag.set(true);
    });
    REQUESTED.with(|total| total.set(0));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TRACK.with(|flag| flag.set(false));
        }
    }
    let reset = Reset;
    let value = operation();
    drop(reset);
    (value, REQUESTED.with(Cell::get))
}
