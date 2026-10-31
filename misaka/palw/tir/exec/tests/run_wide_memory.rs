#![cfg(feature = "node")]
//! Host reservations are logical backing requests, not measured RSS. This
//! binary has one test because the host reserver is installed once per process.
mod run_wide_common;

use kaspa_consensus_core::{Hash64, palw_tir_step_v1::PalwTirStepSpaceV2};
use misaka_palw_tir::DType;
use misaka_palw_tir_exec::{
    NodeValue, Slice,
    node::{
        TirMemoryLeaseV1,
        run::TirRunObserverV1,
        run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
        set_tir_memory_reserver_v1,
    },
};
use run_wide_common::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct HostLease(Arc<AtomicUsize>);

impl Drop for HostLease {
    fn drop(&mut self) {
        assert_eq!(self.0.fetch_sub(1, Ordering::SeqCst), 1);
    }
}

struct Observed {
    active: Arc<AtomicUsize>,
    nodes: usize,
    states: usize,
    outputs: usize,
}

impl Observed {
    fn new(active: &Arc<AtomicUsize>) -> Self {
        Self { active: Arc::clone(active), nodes: 0, states: 0, outputs: 0 }
    }

    fn assert_idle(&self) {
        assert_eq!((self.nodes, self.states, self.outputs), (0, 0, 0));
    }

    fn assert_reserved(&self) {
        assert_eq!(self.active.load(Ordering::SeqCst), 1);
    }
}

impl TirRunObserverV1 for Observed {
    fn node(&mut self, _: u32, _: &NodeValue<'_>) -> Result<(), String> {
        self.assert_reserved();
        self.nodes += 1;
        Ok(())
    }

    fn state(&mut self, _: u32, _: u16, _: Option<u16>, _: Option<u32>, _: DType, _: &[usize], _: Slice<'_>) -> Result<(), String> {
        self.assert_reserved();
        self.states += 1;
        Ok(())
    }

    fn last_output(&mut self, _: u32, _: DType, _: &[usize], _: Slice<'_>, _: u32) -> Result<(), String> {
        self.assert_reserved();
        self.outputs += 1;
        Ok(())
    }
}

#[test]
fn wide_run_reserves_before_execution_and_retains_host_lease_until_drop() {
    // Build the caller-owned plan, params and reference fixture before installing
    // the probe, so only the explicit wide runner requests are recorded.
    with_fixture(3, |wide, _| {
        let space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
        let class = wide.class.class_id(&wide.artifact);
        let context = wide.job.context_hash();
        let runner = TirClassRunnerWideQ24V2::new(&space, wide.plan, wide.params, class).unwrap();
        let mut limits = TirWideRunLimitsV2 {
            max_positions: MAX_CONTEXT,
            max_step_leaves: LEAF_CAP,
            max_row_lanes: 16,
            max_work: 1 << 40,
            max_requested_bytes: 1 << 34,
        };
        let quote = runner.quote(&wide.job, limits).unwrap();
        assert!(quote.requested_bytes > 0);
        println!(
            "LOGICAL_HOST_LEASE requested_bytes={} executor_bytes={} retained_bytes={} scratch_bytes={} leaf_count={} work={} (logical request; not physical RSS)",
            quote.requested_bytes, quote.executor_bytes, quote.retained_bytes, quote.scratch_bytes, quote.leaf_count, quote.work
        );

        let charges = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let reject_next = Arc::new(AtomicBool::new(false));
        let callback_charges = Arc::clone(&charges);
        let callback_active = Arc::clone(&active);
        let callback_reject = Arc::clone(&reject_next);
        assert!(set_tir_memory_reserver_v1(Arc::new(move |role, class, context, bytes| {
            callback_charges.lock().unwrap().push((role, class, context, bytes));
            assert_eq!(callback_active.load(Ordering::SeqCst), 0);
            if callback_reject.swap(false, Ordering::SeqCst) {
                return Err("test host declined wide reservation".into());
            }
            assert_eq!(callback_active.fetch_add(1, Ordering::SeqCst), 0);
            Ok(Box::new(HostLease(Arc::clone(&callback_active))) as TirMemoryLeaseV1)
        })));

        // A local quote ceiling refuses before consulting the host ledger.
        limits.max_requested_bytes = quote.requested_bytes - 1;
        let mut observed = Observed::new(&active);
        let mut leaves = 0;
        assert!(runner.run_observed(&wide.job, PROMPT, context, limits, &mut |_| leaves += 1, &mut observed).is_err());
        assert!(charges.lock().unwrap().is_empty());
        observed.assert_idle();
        assert_eq!(leaves, 0);
        assert_eq!(active.load(Ordering::SeqCst), 0);

        // The exact quoted request is sufficient. Every observed node/state/output
        // and every streamed leaf sees the host reservation already held.
        limits.max_requested_bytes = quote.requested_bytes;
        let run = runner
            .run_observed(
                &wide.job,
                PROMPT,
                context,
                limits,
                &mut |_| {
                    assert_eq!(active.load(Ordering::SeqCst), 1);
                    leaves += 1;
                },
                &mut observed,
            )
            .unwrap();
        assert!(observed.nodes > 0);
        assert!(observed.states > 0);
        assert_eq!(observed.outputs, wide.job.exact_decode_tokens as usize);
        assert_eq!(leaves as u64, quote.leaf_count);
        assert_eq!(run.reserved_bytes(), quote.requested_bytes);
        assert_eq!(run.generated(), EXPECTED_GENERATED);
        assert_eq!(run.logits_rows(), wide.rows);
        assert_eq!(active.load(Ordering::SeqCst), 1, "returned immutable buffers still own the lease");
        println!("LOGICAL_HOST_LEASE active_on_return={}", active.load(Ordering::SeqCst));
        drop(run);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        println!("LOGICAL_HOST_LEASE active_after_drop={}", active.load(Ordering::SeqCst));

        // Binding failure occurs after reservation, releases it, and never starts
        // the observer or leaf stream. The actual context is still charged.
        let mut observed = Observed::new(&active);
        let mut leaves = 0;
        let wrong_context = Hash64::from_u64_word(99);
        assert_ne!(wrong_context, context);
        assert!(runner.run_observed(&wide.job, PROMPT, wrong_context, limits, &mut |_| leaves += 1, &mut observed).is_err());
        observed.assert_idle();
        assert_eq!(leaves, 0);
        assert_eq!(active.load(Ordering::SeqCst), 0);

        // The same once-installed callback can decline another request. Declining
        // produces no run, observations or leaf callbacks, and owns no lease.
        reject_next.store(true, Ordering::SeqCst);
        let error = runner.run_observed(&wide.job, PROMPT, context, limits, &mut |_| leaves += 1, &mut observed).unwrap_err();
        assert!(error.contains("test host declined wide reservation"));
        observed.assert_idle();
        assert_eq!(leaves, 0);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert!(!reject_next.load(Ordering::SeqCst));
        println!("LOGICAL_HOST_LEASE active_after_reject={}", active.load(Ordering::SeqCst));
        assert_eq!(
            *charges.lock().unwrap(),
            vec![("tir-wide-q24-v2", class, context, quote.requested_bytes); 3],
            "success, binding refusal and host refusal use the same exact class/context and logical quote"
        );
    });
}
