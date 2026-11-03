//! Isolated allocator boundary: setup is excluded, successful getter only.
//! Logical request census test; no model execution or host-reservation claim.
use kaspa_consensus_core::palw_tir_step_v1::quote_local_tir_step_space_additional_v1;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
#[path = "common/local_step_geometry_fixture_v1.rs"]
mod fixture;
use kaspa_consensus_core as geometry_core;
struct Probe;
thread_local! {static ACTIVE:Cell<bool>=const{Cell::new(false)};static CALLS:Cell<usize>=const{Cell::new(0)};}
fn count() {
    let _ = ACTIVE.try_with(|a| {
        if a.get() {
            let _ = CALLS.try_with(|n| n.set(n.get() + 1));
        }
    });
}
// SAFETY: all pointers/layouts are forwarded unchanged to System; only
// thread-local scalar counters are updated, without allocating.
unsafe impl GlobalAlloc for Probe {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count();
        // SAFETY: the original valid layout is forwarded unchanged.
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        count();
        // SAFETY: the original valid layout is forwarded unchanged.
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: the original allocated pointer/matching layout are forwarded.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        count();
        // SAFETY: the original pointer/layout and requested new size are forwarded.
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Probe = Probe;
fn allocations<T>(run: impl FnOnce() -> T) -> (T, usize) {
    ACTIVE.with(|a| assert!(!a.get()));
    CALLS.with(|n| n.set(0));
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            ACTIVE.with(|a| a.set(false));
        }
    }
    ACTIVE.with(|a| a.set(true));
    let stop = Stop;
    let value = run();
    let calls = CALLS.with(Cell::get);
    drop(stop);
    (value, calls)
}
#[test]
fn successful_immutable_source_geometry_quote_allocates_zero_after_prepared_setup() {
    let (sentinel, calls) = allocations(|| std::hint::black_box(vec![0u8; 128]));
    assert!(calls > 0);
    drop(sentinel);
    for wide in [false, true] {
        let class = fixture::class(wide, 3);
        let source = fixture::source(&class);
        let expected = quote_local_tir_step_space_additional_v1(&source).unwrap();
        let (actual, calls) = allocations(|| quote_local_tir_step_space_additional_v1(std::hint::black_box(&source)));
        assert_eq!(calls, 0);
        assert_eq!(actual.unwrap(), expected);
    }
}

#[test]
fn successful_fixed_space_diagnostic_quote_allocates_zero() {
    use kaspa_consensus_core::palw_tir_step_v1::quote_local_tir_step_space_fixed_requests_v1;
    let expected = quote_local_tir_step_space_fixed_requests_v1().unwrap();
    let (actual, calls) = allocations(quote_local_tir_step_space_fixed_requests_v1);
    assert_eq!(calls, 0);
    assert_eq!(actual.unwrap(), expected);
}
