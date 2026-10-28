//! Actual blocking-worker and real floor-opening controls. Native-row fixtures
//! authenticate ownership only; they are not Q36 arithmetic or chain campaigns.
use super::*;
use kaspa_consensus_core::{palw_backend::*, palw_segment_resume_v1::PalwSegmentClaimV1, palw_verification_v2::*};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn owner_fixture(free_prompt: bool) -> (Arc<Owner>, Arc<Owner>, Arc<Owner>) {
    crate::palw_panel::fp_replay_routing::tests::native_owner_fixture_v2(free_prompt)
}
struct RecordingBackend {
    expected: Arc<Owner>,
    bound: Option<Arc<Owner>>,
    ready: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl PalwExecutionBackendV1 for RecordingBackend {
    fn model_id(&self) -> &str {
        "segment-routing-owner-control"
    }
    fn job_for_anchor(&self, _: Hash64) -> Result<(kaspa_consensus_core::palw_v2::PalwJobContextV2, Vec<usize>), String> {
        Err("unused".into())
    }
    fn execute(&self, _: &kaspa_consensus_core::palw_v2::PalwJobContextV2, _: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        Err("unused".into())
    }
    fn verify_material(&self, _: &[u8], _: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        PalwMaterialVerdictV1::Unverifiable
    }
    fn bind_native_routing_claim_v2(
        &mut self,
        owner: Arc<Owner>,
    ) -> Result<(), kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1> {
        assert!(Arc::ptr_eq(&owner, &self.expected));
        self.bound = Some(owner);
        Ok(())
    }
    fn replay_segment_from_checkpoint_v1(
        &self,
        job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
        prompt: &[usize],
        _: &[u8],
        _: PalwSegmentClaimV1,
    ) -> Result<kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentReplayV1, String> {
        assert!(self.bound.as_ref().is_some_and(|owner| Arc::ptr_eq(owner, &self.expected)));
        assert_eq!(job, self.expected.job());
        assert_eq!(prompt, &[1]);
        self.ready.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Err("intentional local refusal; no arithmetic success fabricated".into())
    }
}
async fn completed(replays: &mut PalwSeatReplaysV1, key: &PalwSeatReplayKeyV1) -> PalwSeatSegmentPollV1 {
    for _ in 0..2000 {
        let out = replays.poll_segment(key, 12).await;
        if !matches!(out, PalwSeatSegmentPollV1::Running) {
            return out;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("segment worker did not terminate");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn segment_routing_actual_worker_keeps_normal_and_fp_owners_through_detach_and_done() {
    for free_prompt in [false, true] {
        let (original, _, _) = owner_fixture(free_prompt);
        assert_eq!(original.is_free_prompt(), free_prompt);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let backend =
            Box::new(RecordingBackend { expected: original.clone(), bound: None, ready: ready_tx, release: Mutex::new(release_rx) });
        let bound = BoundSegmentReplayV2::bind_with(backend, original.claim(), 0, original.job(), &[1], true, |_, _| {
            Ok(Some(original.clone()))
        })
        .unwrap();
        let weak = Arc::downgrade(&bound.original);
        let key = (original.claim(), h(0x51));
        let mut replays = PalwSeatReplaysV1::default();
        let claim = PalwSegmentClaimV1 { execution_root: h(1), trace_root: h(2), seat_count: 5, segment_index: 0 };
        replays.start_routed_segment_v2(key, h(3), true, 10, None, bound, vec![], claim);
        ready_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        replays.retain_live(|_| false);
        assert!(replays.detached.contains_key(&key));
        assert!(weak.upgrade().is_some());
        // A tick polls the exact detached worker before it is allowed to finish.
        assert!(matches!(replays.poll_segment(&key, 11).await, PalwSeatSegmentPollV1::Running));
        release_tx.send(()).unwrap();
        assert!(matches!(
            completed(&mut replays, &key).await,
            PalwSeatSegmentPollV1::Done { result: Err(PalwSeatSegmentRefusalV1::Local(_)), .. }
        ));
        assert!(Arc::ptr_eq(replays.segment_original_v2(&key).unwrap().authority.as_ref().unwrap(), &original));
        assert!(weak.upgrade().is_some());
        replays.forget_segment(&key);
        assert!(weak.upgrade().is_none());
    }
}
#[test]
fn segment_routing_exact_normal_job_refuses_count_substitution_before_worker() {
    let (original, _, _) = owner_fixture(false);
    let mut substituted = original.job().clone();
    substituted.exact_decode_tokens += 1;
    let result = BoundSegmentReplayV2::bind_with(
        Box::new(crate::palw_panel::seat_s_tests::floor_backend()),
        original.claim(),
        0,
        &substituted,
        &[],
        true,
        |_, _| Ok(Some(original.clone())),
    );
    assert!(matches!(result, Err(SegmentRoutingFaultV2::Stale)));
}

// This control exercises the real arithmetic and real authenticated opening,
// under an explicitly original None decision. It does not label BASE0 as Q36.
#[tokio::test]
async fn segment_routing_real_floor_opening_survives_matched_cache_and_guards_publication() {
    let backend = crate::palw_panel::seat_s_tests::floor_backend();
    let (job, prompt) = backend.job_for_anchor(h(0x71)).unwrap();
    let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, true);
    let run = backend.execute(&job, &prompt).unwrap();
    let (seats, segment, claim_id) = (5, 0, h(0x72));
    let opening = backend.open_segment_checkpoint_v1(&run.material, seats, segment).unwrap();
    let key = palw_seat_s4_candidate_key_v1(claim_id, job.job_id, seats, segment, &opening);
    let bound = BoundSegmentReplayV2::bind_with(Box::new(backend), claim_id, segment, &job, &prompt, true, |_, _| Ok(None)).unwrap();
    let weak = Arc::downgrade(&bound.original);
    let mut replays = PalwSeatReplaysV1::default();
    replays.start_routed_segment_v2(
        key,
        job.shape_profile_id,
        false,
        10,
        None,
        bound,
        opening.clone(),
        PalwSegmentClaimV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            seat_count: seats,
            segment_index: segment,
        },
    );
    assert!(matches!(completed(&mut replays, &key).await, PalwSeatSegmentPollV1::Done { result: Ok(true), .. }));
    let mask = PalwSegmentMaskV2(1 << segment);
    let claim = PalwSeatResumeClaimV1 { claim: claim_id, job_id: job.job_id, seat_count: seats, mask, now_daa: 12 };
    let request = kaspa_consensus_core::palw_segment_resume_v1::palw_segment_opening_request_index_v1(seats, segment);
    let mut openings = HashMap::from([((claim_id, request), vec![opening])]);
    let mut resumes = PalwSeatResumesV1::default();
    assert_eq!(
        resumes.step(&claim, &mut openings, &mut replays, |_, _, _, _| panic!("reuse real completed worker")).await,
        PalwSeatResumeStepV1::Licensed(mask)
    );
    replays.forget_segment(&key); // Matched sidecar now owns the original guard.
    assert!(weak.upgrade().is_some());
    let signatures = AtomicUsize::new(0);
    let attempt_publication =
        |resumes: &mut PalwSeatResumesV1, replays: &mut PalwSeatReplaysV1, result: Result<Option<Arc<Owner>>, String>| {
            if resumes.refresh_routing_with_v2(claim_id, seats, mask, replays, true, true, |_, _| result.clone()).is_ok() {
                signatures.fetch_add(1, Ordering::SeqCst);
            }
        };
    attempt_publication(&mut resumes, &mut replays, Err("native Resource/read unavailable".into()));
    assert_eq!(signatures.load(Ordering::SeqCst), 0);
    assert!(resumes.matched.contains(&(claim_id, segment)));
    assert!(weak.upgrade().is_some());
    attempt_publication(&mut resumes, &mut replays, Ok(None));
    assert_eq!(signatures.load(Ordering::SeqCst), 1);
    let (later_some, _, _) = owner_fixture(false);
    attempt_publication(&mut resumes, &mut replays, Ok(Some(later_some)));
    assert_eq!(signatures.load(Ordering::SeqCst), 1);
    assert!(resumes.matched.is_empty());
    assert!(weak.upgrade().is_none());
    // A missing complete mask must not turn a previous Licensed into permission.
    attempt_publication(&mut resumes, &mut replays, Ok(None));
    assert_eq!(signatures.load(Ordering::SeqCst), 1);
}
#[test]
fn segment_routing_synthetic_matched_some_accepts_forward_progress_and_retires_import_change() {
    let (original, forward, changed) = owner_fixture(false);
    let guard = Arc::new(SegmentRoutingGuardV2 {
        claim: original.claim(),
        segment: 0,
        job: NativeRetainedV1::try_from_clone_v1(original.job()).unwrap(),
        authority: Some(original.clone()),
        _metadata: None,
    });
    // Synthetic matched-state lifetime control only; this creates no replay verdict.
    let mut resumes = PalwSeatResumesV1::default();
    let mut replays = PalwSeatReplaysV1::default();
    let claim = original.claim();
    let mask = PalwSegmentMaskV2(1);
    resumes.matched.insert((claim, 0));
    resumes.routing.remember(guard);
    resumes.refresh_routing_with_v2(claim, 5, mask, &mut replays, true, true, |_, _| Ok(Some(forward.clone()))).unwrap();
    assert!(matches!(
        resumes.refresh_routing_with_v2(claim, 5, mask, &mut replays, true, true, |_, _| Err("read unavailable".into())),
        Err(SegmentRoutingFaultV2::Retry(_))
    ));
    assert!(resumes.matched.contains(&(claim, 0)));
    assert!(matches!(
        resumes.refresh_routing_with_v2(claim, 5, mask, &mut replays, true, true, |_, _| Ok(Some(changed.clone()))),
        Err(SegmentRoutingFaultV2::Stale)
    ));
    assert!(resumes.matched.is_empty());
}
#[test]
fn segment_routing_capacity_refuses_before_clone_or_native_read_and_release_retries() {
    const CHILD: &str = "MISAKA_SEGMENT_ROUTING_CAPACITY_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::segment_routing_capacity_refuses_before_clone_or_native_read_and_release_retries", module_path!());
        let name = name.split_once("::").unwrap().1;
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.lines().any(|line| line.trim() == "running 1 test"), "{stdout}");
        assert!(output.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
        return;
    }
    use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
    let backend = crate::palw_panel::seat_s_tests::floor_backend();
    let (job, prompt) = backend.job_for_anchor(h(0x91)).unwrap();
    let retry_backend = crate::palw_panel::seat_s_tests::floor_backend();
    const LIMIT: u64 = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(LIMIT).unwrap();
    let refused = BoundSegmentReplayV2::bind_with(Box::new(backend), h(0x92), 0, &job, &prompt, true, |_, _| {
        panic!("no native read after refused reservation")
    });
    assert!(matches!(refused, Err(SegmentRoutingFaultV2::Resource(_))));
    drop(blocker);
    let retried = BoundSegmentReplayV2::bind_with(Box::new(retry_backend), h(0x92), 0, &job, &prompt, true, |_, _| Ok(None)).unwrap();
    assert!(reserve_native_requests_v1(LIMIT).is_err());
    drop(retried);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
#[test]
fn segment_routing_production_refresh_precedes_vertex_and_receipt_signing() {
    let source = include_str!("palw_panel/seat_progress.rs");
    let gate = source.find("seat_resumes.refresh_routing_with_v2(").unwrap();
    assert!(gate < source.find("vertex_book.record(vertex::palw_vertex_verdict_leaf_v1(").unwrap());
    assert!(gate < source.find("let message = palw_receipt_message_v3(").unwrap());
    let helper = include_str!("palw_panel.rs");
    let start = helper.find("async fn seat_s4_resume_v1(").unwrap();
    assert!(helper[start..].contains("return PalwSeatResumeStepV1::Waiting;"));
}
