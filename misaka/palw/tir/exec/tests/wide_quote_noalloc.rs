#![cfg(feature = "node")]
//! Success quote allocation boundary for BOTH Panel capture and the direct
//! required runner: they call the same borrowed structural quote. Model/plan/
//! source setup and error/control storage are outside this measurement.
mod run_wide_common;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV2;
use misaka_palw_tir_exec::node::run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
struct AllocationProbe;
thread_local! {static PROBE_ACTIVE:Cell<bool>=const{Cell::new(false)};static PROBE_CALLS:Cell<usize>=const{Cell::new(0)};}
fn record_allocation() {
    let _ = PROBE_ACTIVE.try_with(|active| {
        if active.get() {
            let _ = PROBE_CALLS.try_with(|n| n.set(n.get() + 1));
        }
    });
}
// SAFETY: every operation forwards the original pointers/layout unchanged to
// System. Thread-local integer counters do not allocate and never affect them.
unsafe impl GlobalAlloc for AllocationProbe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: forward the unchanged valid allocation layout to System.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: forward the unchanged valid zeroed allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forward the original allocated pointer and matching layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        // SAFETY: forward the original pointer/layout and requested new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: AllocationProbe = AllocationProbe;
fn allocations<T>(run: impl FnOnce() -> T) -> (T, usize) {
    PROBE_ACTIVE.with(|a| assert!(!a.get()));
    PROBE_CALLS.with(|n| n.set(0));
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            PROBE_ACTIVE.with(|a| a.set(false));
        }
    }
    PROBE_ACTIVE.with(|a| a.set(true));
    let stop = Stop;
    let out = run();
    let count = PROBE_CALLS.with(Cell::get);
    drop(stop);
    (out, count)
}

#[test]
fn successful_wide_quote_needs_no_local_shape_scratch_allocation_before_required_ram_guard() {
    let (sentinel, count) = allocations(|| std::hint::black_box(vec![0u8; 128]));
    assert!(count > 0);
    drop(sentinel);
    for window in [1, 3] {
        run_wide_common::with_fixture(window, |f, _| {
            let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
            let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
            let limits = TirWideRunLimitsV2 {
                max_positions: run_wide_common::MAX_CONTEXT,
                max_step_leaves: run_wide_common::LEAF_CAP,
                max_row_lanes: 16,
                max_work: 1 << 40,
                max_requested_bytes: 1 << 30,
            };
            let expected = runner.quote(&f.job, limits).unwrap();
            let (actual, count) = allocations(|| runner.quote(std::hint::black_box(&f.job), limits));
            assert_eq!(count, 0);
            assert_eq!(actual.unwrap(), expected);
        });
    }
}
