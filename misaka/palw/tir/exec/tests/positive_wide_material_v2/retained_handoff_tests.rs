//! Real ownership transitions; no saved verdict, protocol or native authority.
use super::*;
use misaka_palw_tir_exec::{NodeValue, Slice, node::run::TirRunObserverV1};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
struct Guard(Arc<AtomicU64>, u64);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::SeqCst);
    }
}
#[derive(Default)]
struct Observer(usize);
impl TirRunObserverV1 for Observer {
    fn node(&mut self, _: u32, _: &NodeValue<'_>) -> Result<(), String> {
        self.0 += 1;
        Ok(())
    }
    fn state(
        &mut self,
        _: u32,
        _: u16,
        _: Option<u16>,
        _: Option<u32>,
        _: misaka_palw_tir::DType,
        _: &[usize],
        _: Slice<'_>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn last_output(&mut self, _: u32, _: misaka_palw_tir::DType, _: &[usize], _: Slice<'_>, _: u32) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn exact_handoff_peak_admits_and_one_below_refuses_before_capture_or_cpu_guards() {
    run_wide_common::with_fixture(1, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
        let q = runner.quote(&f.job, run_bounds()).unwrap();
        let peak = q.handoff_peak_requests().unwrap();
        let mut limit = run_bounds();
        limit.max_requested_bytes = peak - 1;
        let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let deny: TirMemoryReserverV1 = Arc::new(|_, _, _, _| panic!("one-below handoff performed a capture/CPU allocation"));
        assert!(
            capture_one_pass_v2(
                CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
                &runner,
                limit,
                &work,
                tempfile::tempfile().unwrap(),
                &deny
            )
            .is_err()
        );
        let spent = work.meter();
        assert!(spent.visits > 0);
        limit.max_requested_bytes = peak;
        assert!(
            capture_one_pass_v2(
                CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
                &runner,
                limit,
                &work,
                tempfile::tempfile().unwrap(),
                &deny
            )
            .is_err()
        );
        assert_eq!(work.meter(), spent, "failed attempt cannot be restarted with a larger ceiling");
        let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let capture = capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
            &runner,
            limit,
            &work,
            tempfile::tempfile().unwrap(),
            &reserve(),
        )
        .unwrap();
        assert_eq!(capture.run().reserved_bytes(), q.retained_output_requests().unwrap());
        assert_eq!(capture.run().generated(), EXPECTED);
    });
}

#[test]
fn output_guard_refusal_after_one_cpu_pass_returns_no_capture_and_spends_the_attempt() {
    run_wide_common::with_fixture(1, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
        let live = Arc::new(AtomicU64::new(0));
        let cpu = Arc::new(AtomicUsize::new(0));
        let denied = Arc::new(AtomicUsize::new(0));
        let (held, runs, outputs) = (live.clone(), cpu.clone(), denied.clone());
        let r: TirMemoryReserverV1 = Arc::new(move |role, _, _, bytes| {
            if role == "tir-wide-q24-v2-required" {
                runs.fetch_add(1, Ordering::SeqCst);
            }
            if role == "tir-wide-q24-v2-retained" {
                assert!(held.load(Ordering::SeqCst) > bytes, "working/capture guards must still cover completed output");
                outputs.fetch_add(1, Ordering::SeqCst);
                return Err("output guard denied".into());
            }
            held.fetch_add(bytes, Ordering::SeqCst);
            Ok(Box::new(Guard(held.clone(), bytes)))
        });
        let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
        let error = capture_one_pass_v2(
            CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
            &runner,
            run_bounds(),
            &work,
            tempfile::tempfile().unwrap(),
            &r,
        )
        .err()
        .unwrap();
        assert!(error.contains("output guard denied"));
        assert_eq!(cpu.load(Ordering::SeqCst), 1);
        assert_eq!(denied.load(Ordering::SeqCst), 1);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        let spent = work.meter();
        assert!(spent.tape_bytes > 0);
        assert!(
            capture_one_pass_v2(
                CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
                &runner,
                run_bounds(),
                &work,
                tempfile::tempfile().unwrap(),
                &r
            )
            .is_err()
        );
        assert_eq!(work.meter(), spent);
        assert_eq!(cpu.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn explicit_retained_mode_preserves_every_output_and_legacy_required_mode_keeps_its_old_guard() {
    run_wide_common::with_fixture(3, |f, _| {
        let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
        let q = runner.quote(&f.job, run_bounds()).unwrap();
        let live = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(Mutex::new(vec![]));
        let (held, seen) = (live.clone(), calls.clone());
        let r: TirMemoryReserverV1 = Arc::new(move |role, _, _, bytes| {
            seen.lock().unwrap().push((role, bytes));
            held.fetch_add(bytes, Ordering::SeqCst);
            Ok(Box::new(Guard(held.clone(), bytes)))
        });
        let mut old_observer = Observer::default();
        let mut leaves = |_: &misaka_palw_tir_exec::node::run_wide_q24_v2::TirLeafOutWideQ24V2<'_>| -> Result<(), String> { Ok(()) };
        let old = runner
            .run_observed_fallible(&f.job, PROMPT, f.job.context_hash(), run_bounds(), &mut leaves, &mut old_observer, &r)
            .unwrap();
        assert_eq!(live.load(Ordering::SeqCst), q.requested_bytes);
        assert_eq!(old.reserved_bytes(), q.requested_bytes);
        let mut new_observer = Observer::default();
        let new = runner
            .run_observed_with_retained_output_v2(
                &f.job,
                PROMPT,
                f.job.context_hash(),
                run_bounds(),
                &mut leaves,
                &mut new_observer,
                &r,
            )
            .unwrap();
        assert_eq!(old_observer.0, new_observer.0);
        assert_eq!(old_observer.0, f.nodes.len());
        assert_eq!(new.generated(), old.generated());
        assert_eq!(new.logits_rows(), old.logits_rows());
        assert_eq!(new.leaf_hashes(), old.leaf_hashes());
        assert_eq!(new.trace_root(), old.trace_root());
        assert_eq!(new.step_merkle_root(), old.step_merkle_root());
        assert_eq!(new.execution_root(), old.execution_root());
        assert_eq!(new.output_root(), old.output_root());
        assert_eq!(live.load(Ordering::SeqCst), q.requested_bytes + q.retained_output_requests().unwrap());
        assert_eq!(calls.lock().unwrap().iter().filter(|(role, _)| *role == "tir-wide-q24-v2-required").count(), 2);
        assert_eq!(calls.lock().unwrap().iter().filter(|(role, _)| *role == "tir-wide-q24-v2-retained").count(), 1);
        drop(new);
        assert_eq!(live.load(Ordering::SeqCst), q.requested_bytes);
        drop(old);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    });
}
