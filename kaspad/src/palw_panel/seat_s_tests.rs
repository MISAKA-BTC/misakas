//! **`palw_panel`'s `seat_s_tests` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

//! **The audit's SEAT-S review, fixed past SEAT-R and pinned by finding** (C1, H1–H3, M1–M3,
//! L1, L2), with the deadline design's N-5 (the material wait) and R2 (retention to `R_eff`), and
//! the re-enabled SEAT-S4 route: a C7 partial seat resumes its own mask from served openings the
//! family authenticates against the claim, off the loop, and files that mask's V3 alone. The
//! review's second pass is pinned as `f1_*`–`f6_*`. Below SEAT-R every path, form and deadline is
//! what it was.
use super::{
    PALW_SEAT_MATERIAL_WAIT_CAP_DAA_V1, PALW_SEAT_RETENTION_LIABILITIES_MAX_V1,
    PALW_SEAT_S4_AUTHENTICATION_REFUSALS_V1, PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1, PalwReplayAnswerV1, PalwSeatArmV1,
    PalwSeatClassReadV1, PalwSeatRRoleV1, PalwSeatReceiptFormsV1, PalwSeatReplayPollV1, PalwSeatReplayStepV1, PalwSeatReplaysV1,
    PalwSeatResumeClaimV1, PalwSeatResumeStepV1, PalwSeatResumesV1, PalwSeatSegmentPollV1, PalwSeatSegmentRefusalV1, PalwSeatTailV1,
    palw_replay_answer_v1, palw_seat_arm_licenses_v1, palw_seat_material_wait_ends_v1, palw_seat_material_wait_until_v1,
    palw_seat_note_duty_liability_v1, palw_seat_note_liability_v1, palw_seat_r_role_v1, palw_seat_receipt_forms_v1,
    palw_seat_replay_step_v1, palw_seat_retention_horizon_v1, palw_seat_retention_pins_v1, palw_seat_s4_candidate_key_v1,
    palw_seat_s4_refusal_v1, palw_seat_tail_v1, palw_seat_unserved_licence_v1, prune_foreign_retention_v1,
};
use kaspa_consensus_core::palw_backend::{PalwExecutionBackendV1, PalwReplayRootsV1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_segment_resume_v1::palw_segment_opening_request_index_v1;
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
use kaspa_consensus_core::palw_verification_v2::{PalwSegmentMaskV2, palw_segment_assignment_v2, palw_segment_count_v2};
use kaspa_hashes::Hash64;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

pub(super) fn floor_backend() -> misaka_palw_base0::backend::Base0Backend {
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    let court =
        kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .expect("court");
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("floor");
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("root");
    misaka_palw_base0::backend::Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves"))
        .with_step_ladder_cap(court.max_step_leaf_count())
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
}

/// The source before the first test module: what the pins read.
fn source() -> &'static str {
    crate::palw_panel::source_support::former_parent_production_source_v1()
}

fn body_of<'a>(source: &'a str, head: &str) -> &'a str {
    let from = source.find(head).unwrap_or_else(|| panic!("{head}"));
    let rest = &source[from..];
    &rest[..rest.find("\n    }\n").map(|end| end + 6).unwrap_or(rest.len())]
}

/// A free-prompt job of the floor's, greedy, five prompt ids and a seven-token ceiling, and the ids.
pub(super) fn floor_fp_job(
    backend: &misaka_palw_base0::backend::Base0Backend,
) -> (kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3, Vec<u32>) {
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
    };
    let form = backend.prompt_ids_form();
    let ids = vec![3u32, 1, 4, 1, 5];
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: h(0xD0),
        class_id: backend.profile().shape_profile_id(),
        executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0),
            0,
        ),
        executor_pubkey: vec![0x11; 32],
        operator_id: h(0x0B),
        anchor_block: h(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(form, &ids).expect("commits"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: 7,
        max_context_tokens: backend.profile().n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    (job, ids)
}

/// A five-seat claim's partial seat and the one segment its mask covers.
fn partial_claim(now_daa: u64) -> (PalwSeatResumeClaimV1, u16, u32) {
    let (anchor, claim, seats) = (h(0xA1), h(0xC1), 5u16);
    let assignment = palw_segment_assignment_v2(anchor, claim, seats);
    let seat = (0..seats).find(|s| *s != assignment.full_seat).expect("a partial seat");
    let mask = assignment.mask_of(seat);
    let k = palw_segment_count_v2(seats);
    assert_eq!(mask.count(k), 1, "t12's five seats: one segment a partial seat");
    let segment = (0..k).find(|i| mask.covers(*i)).expect("its segment");
    (
        PalwSeatResumeClaimV1 { claim, job_id: h(0x10B), seat_count: seats, mask, now_daa },
        segment,
        palw_segment_opening_request_index_v1(seats, segment),
    )
}

/// A segment task that answers from the opening's bytes alone — `real` roots, `forged` (the
/// real binding with a sibling path of the forger's) does not, anything else is not an opening —
/// counting the starts.
fn fake_start(
    starts: &Arc<std::sync::atomic::AtomicUsize>,
) -> impl FnMut(&mut PalwSeatReplaysV1, (Hash64, Hash64), u16, Vec<u8>) -> bool {
    let starts = starts.clone();
    move |replays, key, _segment, opening| {
        if !replays.has_room(true) {
            return false;
        }
        starts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        replays.start_segment(key, h(0x2E), true, 0, None, Box::new(floor_backend()), move |_| match opening.as_slice() {
            b"real" => Ok(true),
            forged if forged.starts_with(b"forged") => Ok(false),
            _ => Err(PalwSeatSegmentRefusalV1::NotTheClaims("NotAnOpening".into())),
        });
        true
    }
}

/// Step until nothing runs: the verdict of the tick after every started task returned.
async fn settle(
    resumes: &mut PalwSeatResumesV1,
    claim: &PalwSeatResumeClaimV1,
    openings: &mut HashMap<(Hash64, u32), Vec<Vec<u8>>>,
    replays: &mut PalwSeatReplaysV1,
    starts: &Arc<std::sync::atomic::AtomicUsize>,
) -> PalwSeatResumeStepV1 {
    for _ in 0..400 {
        let step = resumes.step(claim, openings, replays, fake_start(starts)).await;
        if step != PalwSeatResumeStepV1::Waiting {
            return step;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the resume never settled")
}

/// [`settle`] with the caller's own task.
async fn settle_with(
    resumes: &mut PalwSeatResumesV1,
    claim: &PalwSeatResumeClaimV1,
    openings: &mut HashMap<(Hash64, u32), Vec<Vec<u8>>>,
    replays: &mut PalwSeatReplaysV1,
    mut start: impl FnMut(&mut PalwSeatReplaysV1, (Hash64, Hash64), u16, Vec<u8>) -> bool,
) -> PalwSeatResumeStepV1 {
    for _ in 0..2_000 {
        let step = resumes.step(claim, openings, replays, &mut start).await;
        if step != PalwSeatResumeStepV1::Waiting {
            return step;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("the resume never settled")
}

/// **C1: past SEAT-R, S3 never attests, and no pooled capture is believed.** The legacy resume
/// abstains past the fence before it reads a byte (its own pin in `seat_r_tests`); the SEAT-S4
/// route reads no pooled capture, samples no site and records no fault from what it is served,
/// and the one arm past the fence that licenses a partial mask is its own.
#[test]
fn c1_s3_never_attests_past_the_fence_without_verify_material() {
    let source = source();
    let route = body_of(source, "    async fn seat_s4_resume_v1(");
    let step = body_of(source, "    pub(crate) async fn step(");
    for body in [route, step] {
        for never in ["replay_layer_site_v3", "open_segment_checkpoint_v1", "palw_layer_sample_v3", "note_seat_fault_v1", "materials"]
        {
            assert!(!body.contains(never), "the SEAT-S4 route reads no {never}");
        }
    }
    assert!(route.contains(".replay_segment_from_checkpoint_v1(&ctx, &prompt, &opening, segment_claim)"), "authenticated first");
    assert!(route.contains("execution_root: duty.execution_root") && route.contains("trace_root: duty.trace_root"));
    assert!(!palw_seat_arm_licenses_v1(true, PalwSeatArmV1::SegmentResume), "the legacy S1/S3 resume licenses nothing");
    assert!(palw_seat_arm_licenses_v1(true, PalwSeatArmV1::VerifiedSegmentResume));
    let legacy = body_of(source, "    async fn palw_v2_try_partial_resume_v1(");
    let abstain = legacy.find("return PalwV2SeatPathV1::Abstain;").expect("the abstention");
    assert!(abstain < legacy.find("palw_verification_s3_at(current_daa)").expect("S3"), "S3 is below the fence only");
}

/// **H1: a junk or forged opening neither stalls nor faults an honest claim.** Junk replies fill
/// the pair; every one is tried, refused, dropped from the pool (which frees the transport's
/// slot) and the opening asked for again — and junk alone is not being served (`served: false`,
/// the material wait's N-5 case). A forged sibling path (`Ok(false)` from an authenticated
/// opening) is dropped the same way and is never a fault; it was authenticated as the claim's, so
/// the seat counts as served and never accuses (`served: true`). Bytes re-served after a refusal
/// cost no second replay; and the real opening, arriving after them, licenses the mask.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h1_a_junk_or_forged_opening_does_not_stall_or_fault_an_honest_claim() {
    let (claim, _segment, request) = partial_claim(100);
    let (mut resumes, mut replays) = (PalwSeatResumesV1::default(), PalwSeatReplaysV1::default());
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
    openings.insert((claim.claim, request), vec![b"junk1".to_vec(), b"junk2".to_vec(), b"12345".to_vec()]);
    let step = settle(&mut resumes, &claim, &mut openings, &mut replays, &starts).await;
    assert_eq!(
        step,
        PalwSeatResumeStepV1::Starved { missing: vec![request], served: false },
        "every junk candidate tried, then asked again; junk is not being served"
    );
    assert!(!openings.contains_key(&(claim.claim, request)), "the refused are dropped: the slot is free");
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 3, "each tried once");
    openings.insert((claim.claim, request), vec![b"forged".to_vec(), b"junk4".to_vec()]);
    let step = settle(&mut resumes, &claim, &mut openings, &mut replays, &starts).await;
    assert_eq!(
        step,
        PalwSeatResumeStepV1::Starved { missing: vec![request], served: true },
        "a forged sibling path is dropped, asked for again, and no accusation follows it"
    );
    assert!(!openings.contains_key(&(claim.claim, request)), "the forged is dropped too");
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 5);
    // The same junk re-served: its verdict stands, no second replay.
    openings.insert((claim.claim, request), vec![b"junk1".to_vec()]);
    let step = settle(&mut resumes, &claim, &mut openings, &mut replays, &starts).await;
    assert_eq!(step, PalwSeatResumeStepV1::Starved { missing: vec![request], served: true });
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 5, "no second replay of bytes already refused");
    // The honest opening arrives behind them.
    openings.insert((claim.claim, request), vec![b"junk3".to_vec(), b"real".to_vec()]);
    let step = settle(&mut resumes, &claim, &mut openings, &mut replays, &starts).await;
    assert_eq!(step, PalwSeatResumeStepV1::Licensed(claim.mask), "the mask's V3, from the one opening that roots");
    assert_eq!(
        palw_seat_receipt_forms_v1(true, true, true, PalwSeatRRoleV1::PartialResumes, claim.mask),
        PalwSeatReceiptFormsV1 { segments: Some(claim.mask), whole: false },
        "its own mask's V3 alone, never the whole-job V2"
    );
}

/// **H1 through the floor's own family**: the route's task is `replay_segment_from_checkpoint_v1`
/// itself, and the pool holds five bytes of junk, the producer's real opening with one sibling of
/// its path forged, and — behind them — the real opening. The junk is refused (`Err`) and the
/// forged path authenticates and does not root (`Ok(false)`); both are dropped and neither is a
/// fault; the real opening licenses the seat's own mask. A claim whose roots are not the capture's
/// refuses the real opening too: no opening of another run's capture licenses anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h1_the_floor_family_licenses_the_real_opening_behind_junk_and_a_forged_path() {
    use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
    use kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1;
    use misaka_palw_base0::segment_opening::Base0SegmentOpeningV2;
    let backend = floor_backend();
    let (job, prompt) = backend.job_for_anchor(h(0x00C0_FFEE)).expect("job");
    let job = palw_attempt_job_v1(job, true);
    let run = backend.execute(&job, &prompt).expect("the producer's run");
    let (seats, claim_id, anchor) = (5u16, h(0xC5), h(0xA5));
    let assignment = palw_segment_assignment_v2(anchor, claim_id, seats);
    let seat = (0..seats).find(|s| *s != assignment.full_seat).expect("a partial seat");
    let mask = assignment.mask_of(seat);
    let k = palw_segment_count_v2(seats);
    let segment = (0..k).find(|i| mask.covers(*i)).expect("its segment");
    let request = palw_segment_opening_request_index_v1(seats, segment);
    let real = backend.open_segment_checkpoint_v1(&run.material, seats, segment).expect("the producer opens its segment");
    let mut forged = Base0SegmentOpeningV2::decode_v2(&real).expect("decodes");
    assert!(!forged.proof.siblings.is_empty(), "a five-seat segment has a path to forge");
    forged.proof.siblings[0] = h(0x5B);
    let forged = forged.encode_v2().expect("encodes");
    let claim = PalwSeatResumeClaimV1 { claim: claim_id, job_id: job.job_id, seat_count: seats, mask, now_daa: 100 };
    let start_for = |execution_root: Hash64, trace_root: Hash64| {
        let (job, prompt) = (job.clone(), prompt.clone());
        move |replays: &mut PalwSeatReplaysV1, key, segment_index: u16, opening: Vec<u8>| {
            if !replays.has_room(true) {
                return false;
            }
            let (job, prompt) = (job.clone(), prompt.clone());
            let segment_claim = PalwSegmentClaimV1 { execution_root, trace_root, seat_count: seats, segment_index };
            replays.start_segment(key, h(0x2E), true, 100, None, Box::new(floor_backend()), move |b| {
                b.replay_segment_from_checkpoint_v1(&job, &prompt, &opening, segment_claim)
                    .map(|replay| replay.matches)
                    .map_err(palw_seat_s4_refusal_v1)
            });
            true
        }
    };
    let (mut resumes, mut replays) = (PalwSeatResumesV1::default(), PalwSeatReplaysV1::default());
    let mut openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
    // A claim whose roots are not this capture's: the real opening is refused, and junk too.
    let stranger = PalwSeatResumeClaimV1 { claim: h(0xC6), ..claim };
    openings.insert((stranger.claim, request), vec![b"12345".to_vec(), real.clone()]);
    assert_eq!(
        settle_with(&mut resumes, &stranger, &mut openings, &mut replays, start_for(h(0xBAD), run.trace_root)).await,
        PalwSeatResumeStepV1::Starved { missing: vec![request], served: false },
        "an opening of another run's capture is not this claim's"
    );
    // The honest claim: junk, the forged path, then the real opening.
    openings.insert((claim.claim, request), vec![b"12345".to_vec(), forged.clone()]);
    assert_eq!(
        settle_with(&mut resumes, &claim, &mut openings, &mut replays, start_for(run.execution_root, run.trace_root)).await,
        PalwSeatResumeStepV1::Starved { missing: vec![request], served: true },
        "the forged path authenticated, did not root, and was dropped — asked for again, and no accusation"
    );
    assert!(!openings.contains_key(&(claim.claim, request)));
    openings.insert((claim.claim, request), vec![forged, real]);
    assert_eq!(
        settle_with(&mut resumes, &claim, &mut openings, &mut replays, start_for(run.execution_root, run.trace_root)).await,
        PalwSeatResumeStepV1::Licensed(mask),
        "the real opening, behind them, licenses the seat's own mask"
    );
}

/// **H2: a held segment replay never blocks the loop.** A C7 segment task that runs for as long
/// as its gate holds is polled, not awaited: the tick returns at once with `Waiting`, the heavy
/// slot is held and a light replay still starts (a floor claim, a court turn's close and a
/// possession proof are the loop's, not the task's), and the verdict arrives on a later tick.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h2_the_held_partial_replay_runs_off_the_loop() {
    let (claim, _segment, request) = partial_claim(100);
    let (mut resumes, mut replays) = (PalwSeatResumesV1::default(), PalwSeatReplaysV1::default());
    let mut openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
    openings.insert((claim.claim, request), vec![b"real".to_vec()]);
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let start = |replays: &mut PalwSeatReplaysV1, key, _segment, _opening: Vec<u8>| {
        let gate = gate.clone();
        replays.start_segment(key, h(0x2E), true, 0, None, Box::new(floor_backend()), move |_| {
            gate.lock().unwrap().recv().map_err(|e| PalwSeatSegmentRefusalV1::Local(e.to_string()))?;
            Ok(true)
        });
        true
    };
    let tick = tokio::time::timeout(std::time::Duration::from_secs(5), resumes.step(&claim, &mut openings, &mut replays, start))
        .await
        .expect("the tick returns while the replay runs");
    assert_eq!(tick, PalwSeatResumeStepV1::Waiting, "nothing filed, and never the material wait's Unavailable");
    assert!(!replays.has_room(true), "the C7 slot is held");
    assert!(replays.has_room(false), "a light replay still starts beside it");
    let again = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        resumes.step(&claim, &mut openings, &mut replays, |_: &mut PalwSeatReplaysV1, _, _, _| panic!("one candidate in flight")),
    )
    .await
    .expect("polled, not awaited");
    assert_eq!(again, PalwSeatResumeStepV1::Waiting);
    release.send(()).unwrap();
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    assert_eq!(settle(&mut resumes, &claim, &mut openings, &mut replays, &starts).await, PalwSeatResumeStepV1::Licensed(claim.mask));
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 0, "returned, not re-run");
    // The route puts every segment replay in the slots and prices it as the streamed fold.
    let route = body_of(source(), "    async fn seat_s4_resume_v1(");
    assert!(route.contains("replays.start_segment(key, duty.class_id, seat_r_duty.heavy, current_daa, Some(reserved), backend,"));
    assert!(route.contains("crate::palw_backends::palw_partial_seat_streamed_need_v1("), "M3");
    assert!(route.contains("if !replays.has_room(seat_r_duty.heavy) {"));
}

/// **H3: the resume uses an opening that arrives after the first tick** — asked for on the first,
/// started on the tick it arrives, licensed on the tick it returns; and bounded: a segment spends
/// at most [`PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1`] candidates, after which it is not asked for
/// again and the seat holds nothing to try.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h3_the_attempt_partial_resume_uses_an_opening_that_arrives_after_the_first_tick() {
        use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let (claim, _segment, request) = partial_claim(100);
    let (mut resumes, mut replays) = (PalwSeatResumesV1::default(), PalwSeatReplaysV1::default());
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
    let first = resumes.step(&claim, &mut openings, &mut replays, fake_start(&starts)).await;
    assert_eq!(first, PalwSeatResumeStepV1::Starved { missing: vec![request], served: false }, "the first tick asks");
    openings.insert((claim.claim, request), vec![b"real".to_vec()]);
    let later = PalwSeatResumeClaimV1 { now_daa: 130, ..claim };
    assert_eq!(settle(&mut resumes, &later, &mut openings, &mut replays, &starts).await, PalwSeatResumeStepV1::Licensed(claim.mask));
    // Bounded: junk refused before a step replays spends none of the bound, and the seat keeps
    // asking; forged openings each spend a replay, and past the bound the segment is not asked for
    // again and the seat holds nothing to try.
    let other = PalwSeatResumeClaimV1 { claim: h(0xC2), ..claim };
    let spent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for round in 0..2 * PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1 {
        openings.insert((other.claim, request), vec![format!("junk{round}").into_bytes()]);
        let step = settle(&mut resumes, &other, &mut openings, &mut replays, &spent).await;
        assert_eq!(
            step,
            PalwSeatResumeStepV1::Starved { missing: vec![request], served: false },
            "junk round {round}: still asking, and junk is not being served"
        );
    }
    let mut round = 0u32;
    loop {
        openings.insert((other.claim, request), vec![format!("forged{round}").into_bytes()]);
        let step = settle(&mut resumes, &other, &mut openings, &mut replays, &spent).await;
        round += 1;
        if step == (PalwSeatResumeStepV1::Starved { missing: vec![], served: true }) {
            break;
        }
        assert_eq!(step, PalwSeatResumeStepV1::Starved { missing: vec![request], served: true });
        assert!(round <= PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1 + 1, "bounded");
    }
    assert_eq!(
        round, PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1,
        "the bound of replayed candidates, then nothing to try — and, served, no accusation"
    );
    // The old per-duty resume (`replayed.insert`) is below SEAT-R only: past it the attempt lane's
    // C7 partial seat takes the SEAT-S4 route every tick.
    let source = source();
    let block = &source[source.find_code("let verdict = 'verdict: {").unwrap()..];
    let fence = block.find_code("let resolved = if seat_r {").expect("the attempt lane's fence");
    let resumed = fence + block[fence..].find_code(".seat_s4_resume_v1(").expect("the S4 route");
    let old = fence + block[fence..].find_code("&& replayed.insert(seat_duty_panel_key_v1(duty))").expect("the old path");
    assert!(resumed < old, "the S4 route is the fence's branch; the once-per-duty resume its else");
}

/// **M2: a replay whose answer is not the claim's licenses nothing.** SEAT-S2's comparison: the
/// floor's honest replay reproduces the committed output root and licenses; a claim whose roots
/// are the honest run's and whose output root names other tokens is refuted (terminal for
/// `Valid`; the full seat's fault finders run on); a replay naming no answer is no verdict.
#[test]
fn m2_an_output_root_mismatch_is_no_valid() {
    use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
    let backend = floor_backend();
    let (job, prompt) = backend.job_for_anchor(h(0x00C0_FFEE)).expect("job");
    let job = palw_attempt_job_v1(job, true);
    let claim = backend.execute(&job, &prompt).expect("the producer's run");
    let replayed = backend.execute_for_verdict(&job, &prompt);
    assert_eq!(replayed.as_ref().expect("replays").output_root, Some(claim.output_root), "the family fills it");
    let step = |output| palw_seat_replay_step_v1(&replayed, claim.execution_root, claim.trace_root, 0, output);
    assert_eq!(step(claim.output_root), PalwSeatReplayStepV1::Licensed);
    assert_eq!(step(h(0xBAD)), PalwSeatReplayStepV1::Refuted, "the roots are the honest run's, the answer is not");
    let unnamed = PalwReplayRootsV1 { output_root: None, ..replayed.clone().unwrap() };
    assert_eq!(palw_replay_answer_v1(&unnamed, claim.output_root), PalwReplayAnswerV1::Unnamed);
    assert_eq!(
        palw_seat_replay_step_v1(&Ok(unnamed), claim.execution_root, claim.trace_root, 0, claim.output_root),
        PalwSeatReplayStepV1::NoVerdict,
        "no answer to compare: nothing licensed, nothing refuted"
    );
    // The free-prompt lane: the claim commits the run's own output root
    // (`palw_fp_commitment_from_context_v3`, under the executed context), and the FP pass's replay
    // names the run's own — the same value, so the honest claim licenses and a claim whose roots
    // are the honest run's and whose answer is not is refuted.
    let (fp_job, ids) = floor_fp_job(&backend);
    let fp_prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
    let run = backend.execute_free_prompt(&fp_job, &fp_prompt).expect("the producer's free-prompt run");
    let ctx = backend.fp_job_context_for_executed_v1(&fp_job, run.facts.decode_tokens_executed).expect("the executed context");
    let committed = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3(&fp_job, &ctx, &run, 0)
        .expect("the claim the lane commits");
    let fp_replayed = backend.execute_free_prompt(&fp_job, &fp_prompt).map(|run| PalwReplayRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.facts.full_logits_trace_root,
        work_leaves: Some(run.facts.step_leaf_count),
        output_root: Some(run.outcome.output_root),
    });
    let fp_step =
        |output| palw_seat_replay_step_v1(&fp_replayed, committed.execution_root, committed.trace_root, committed.work_leaves, output);
    assert_eq!(fp_step(committed.output_root), PalwSeatReplayStepV1::Licensed, "the honest free-prompt claim");
    assert_eq!(fp_step(h(0xBAD)), PalwSeatReplayStepV1::Refuted, "its roots, another answer");
    // Both off-loop replays compare it: the attempt pass and the free-prompt pass, whose replay
    // names the run's own output root.
    let source = source();
    for head in ["    async fn attempt_seat_replay_v1(", "    async fn fp_seat_replay_pass_v1("] {
        assert!(body_of(source, head).contains("duty.output_root"), "{head} compares the claim's output root");
    }
    // The FP pass hands its exact bound backend to the production worker;
    // the connected helper now projects the run's original output root.
    assert!(body_of(source, "    async fn fp_seat_replay_pass_v1(").contains("replays.start_fp_routed_v2("));
    assert!(body_of(source, "    fn start_fp_routed_v2(").contains("bound.execute(&job,&prompt)"));
    let routed = include_str!("../palw_fp_replay_routing_v2.rs");
    let execute = body_of(routed, "    pub(super) fn execute(");
    assert!(execute.contains("self.backend.execute_free_prompt(job, prompt)"));
    assert!(execute.contains("shape.job_context != *original.job"), "the actual executed context remains bound");
    assert!(execute.contains("output_root: Some(run.outcome.output_root),"));
}

/// **M3: a partial seat is priced as the streamed fold it keeps**, not 72 B for every leaf it
/// walks — hundreds of GB at a 2M segment, which no host reserves.
#[test]
fn m3_a_partial_seat_is_priced_as_the_streamed_fold() {
    use crate::palw_backends::{PalwRoleMemoryNeedV1, palw_partial_seat_streamed_fold_bytes_v1, palw_partial_seat_streamed_need_v1};
    use kaspa_consensus_core::palw_resource_profile_v1::{
        PalwCaptureRetentionV1, PalwResourceProfileV1, PalwResourceRoleV1, PalwRuntimeProfileV1,
    };
    const MIB: u64 = 1 << 20;
    let two_m_segment = 3u64 << 32; // a 2M job's leaves to a late segment's end: ~1.3e10
    let eight_k = 1u64 << 21;
    // Under d3ff09371's structural host bound (eight retained and four block hashes an entry, `palw_fold_capture_bytes_v1`) a 2M
    // segment's fold is 1.51 GiB — it was 389 MiB under the two-hash estimate — still under 2 GiB against the profile's ~864 GiB.
    assert_eq!(palw_partial_seat_streamed_fold_bytes_v1(two_m_segment), 1_616_904_192, "a 2M segment folds in 1.51 GiB");
    assert!(palw_partial_seat_streamed_fold_bytes_v1(two_m_segment) < 2048 * MIB);
    assert!(palw_partial_seat_streamed_fold_bytes_v1(eight_k) < 8 * MIB, "an 8k segment in a few MiB");
    assert!(two_m_segment * 72 > 500 * 1024 * MIB, "the profile's figure: hundreds of GB");
    let profile = |role, capture, leaves: u64| PalwResourceProfileV1 {
        runtime: PalwRuntimeProfileV1::A16KvI16,
        role,
        attention_layers: 28,
        kv_dim: 256,
        heads: 12,
        resume_rows: 0,
        end_rows: 1_000,
        kv_resident_bytes: 7,
        checkpoint_bytes: 0,
        opening_bytes: 0,
        retained_checkpoint_bytes: 0,
        attention_scratch_bytes: 0,
        trace_scratch_bytes: 0,
        capture,
        capture_retained_bytes: leaves.saturating_mul(72),
        checkpoint_leg_bytes: 0,
        leaves,
        recurrence_layers: 0,
        gdn_state_bytes: 0,
    };
    let need = |role, capture| PalwRoleMemoryNeedV1 {
        role,
        holding_bytes: 11,
        pinned_bytes: 0,
        prefill_run_positions: None,
        derived_bytes: 0,
        runtime: None,
        profile: Some(profile(role, capture, two_m_segment)),
    };
    let partial = PalwResourceRoleV1::PartialSeat { seat_count: 5, segment_index: 3 };
    let repriced = palw_partial_seat_streamed_need_v1(need(partial, PalwCaptureRetentionV1::ReplayHashes));
    let p = repriced.profile.expect("kept");
    assert_eq!(p.capture_retained_bytes, palw_partial_seat_streamed_fold_bytes_v1(two_m_segment));
    assert_eq!((p.kv_resident_bytes, repriced.holding_bytes), (7, 11), "every other term is the profile's");
    let full = need(PalwResourceRoleV1::FullSeat, PalwCaptureRetentionV1::Fold { retain_level: 12 });
    assert_eq!(palw_partial_seat_streamed_need_v1(full.clone()), full, "another role is returned as it came");
}

/// **L1: a partial seat whose mask is the whole job replays it** — two seats cut one segment — on
/// every read class, C7 included; unread stays unread; and a five-seat panel's partial seats keep
/// their roles.
#[test]
fn l1_a_full_mask_partial_seat_takes_the_full_replay() {
    use PalwSeatRRoleV1::*;
    let (anchor, claim) = (h(0xA2), h(0xC2));
    let two = palw_segment_assignment_v2(anchor, claim, 2);
    let partial = (0..2u16).find(|s| *s != two.full_seat).expect("the other seat");
    assert!(two.mask_of(partial).is_full(palw_segment_count_v2(2)), "one segment: the partial mask is the job");
    for held in [Some(true), Some(false)] {
        assert_eq!(palw_seat_r_role_v1(false, held, false, true), PartialReplays, "{held:?}");
    }
    assert_eq!(palw_seat_r_role_v1(false, None, false, true), PartialAbstains);
    let five = palw_segment_assignment_v2(anchor, claim, 5);
    for seat in (0..5u16).filter(|s| *s != five.full_seat) {
        assert!(!five.mask_of(seat).is_full(palw_segment_count_v2(5)), "no effect on t12's five seats");
    }
}

/// **L2: a free-prompt seat's context is its answer's** — the job the answer's ids bound under, at
/// the count its length names, never the job's ceiling (an early stop's context is not the
/// ceiling's) — and a seat holding no answer derives none and asks for the claim's material,
/// whose answer envelope carries the ids (the second pass, finding 1, retired the price search:
/// a held job the answer did not bind is anybody's).
#[test]
fn l2_an_fp_seat_without_the_output_ids_derives_no_context_and_asks() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let backend = floor_backend();
    let (job, _ids) = floor_fp_job(&backend);
    let ceiling = backend.fp_job_context_for_executed_v1(&job, 7).expect("the ceiling's context");
    let early = backend.fp_job_context_for_executed_v1(&job, 5).expect("an early stop's context");
    assert_ne!(early, ceiling, "an early stop's context is not the ceiling's");
    let source = source();
    let context = body_of(source, "    fn fp_seat_job_v1(");
    assert!(context.contains_code("let (material, ids) = self.fp_committed_answer_v1(backend, duty, held)?;"), "the answer's job only");
    assert!(context.contains_code("backend.fp_job_context_for_executed_v1(&material.job, ids.len()"), "at the answer's length");
    assert!(!context.contains_code("fp_job_context_v1(&material.job)"), "never the ceiling");
    assert!(!context.contains_code("fp_job_material_for_claim("), "never a held job the answer did not bind");
    // The M2 Phase 3 review's H-2: the context must reproduce the pin the claim recorded, or a
    // same-shaped job J′ whose answer authenticates is resumed under J′ and convicted by J1-FP.
    assert!(context.contains_code("if let Some(pin) = duty.fp_job_pin_v1()"), "H-2: the recorded pin is read off the duty");
    assert!(
        context.contains_code("palw_fp_job_pin_of_context_v1(&ctx) != pin") && context.contains_code("return None;"),
        "H-2: a context whose pin is not the claim's resumes nothing"
    );
    let block = &source[source.find_code("let verdict = 'verdict: {").unwrap()..];
    let unanswered = block.find_code("// No job the answer authenticated, so no context to authenticate an").expect("the route's None arm");
    let arm = &block
        [unanswered..unanswered + block[unanswered..].find_code("PalwSeatResumeStepV1::Starved { missing: Vec::new(), served }").unwrap()];
    assert!(arm.contains_code("self.request_material_signed(network_domain, duty.claim_id, current_daa).await;"), "it asks");
}

/// **F1: a stranger's free-prompt job at the claim's price refuses the producer's honest opening**
/// (the second pass's review probe, adopted). The duty's class and executor, another nonce and
/// another prompt of the same length price the same, and under that job the family refuses the
/// honest opening as another job's — a `NotTheClaims` refusal, which drops the bytes and leaves
/// the seat unserved. So the route resumes only under the job the answer authenticates
/// (`fp_seat_job_v1`), under which the same opening licenses the seat's mask.
#[test]
fn f1_a_strangers_fp_job_at_the_claims_price_refuses_the_honest_opening() {
    use kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3;
    use kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1;
    let backend = floor_backend();
    let (job, ids) = floor_fp_job(&backend);
    let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
    let run = backend.execute_free_prompt(&job, &prompt).expect("the producer's run");
    let executed = run.facts.decode_tokens_executed;
    let ctx = backend.fp_job_context_for_executed_v1(&job, executed).expect("the executed context");
    let price = backend.fp_context_work_leaves_v1(&ctx).expect("a price");
    let committed =
        kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3(&job, &ctx, &run, 0).expect("the claim");
    assert_eq!(committed.work_leaves, price, "the claim's work_leaves is the priced context's");
    let (seats, segment) = (5u16, 1u16);
    let opening = backend.open_segment_checkpoint_v1(&run.outcome.material, seats, segment).expect("the producer opens");
    let claim = PalwSegmentClaimV1 {
        execution_root: committed.execution_root,
        trace_root: committed.trace_root,
        seat_count: seats,
        segment_index: segment,
    };
    let honest = backend.replay_segment_from_checkpoint_v1(&ctx, &prompt, &opening, claim).expect("the claim's own job");
    assert!(honest.matches, "under the claim's own job the honest opening licenses the seat's mask");
    let stranger_ids = [9u32, 2, 6, 5, 3];
    let stranger_prompt: Vec<usize> = stranger_ids.iter().map(|t| *t as usize).collect();
    let form = backend.prompt_ids_form();
    let stranger = PalwFreePromptJobV3 {
        job_nonce: [0x77; 32],
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(form, &stranger_ids)
            .expect("commits"),
        ..job.clone()
    };
    let stranger_ctx = backend.fp_job_context_for_executed_v1(&stranger, executed).expect("a context");
    assert_eq!(backend.fp_context_work_leaves_v1(&stranger_ctx), Some(price), "the stranger's job prices at the claim's");
    assert_ne!(stranger_ctx, ctx);
    let refused = backend
        .replay_segment_from_checkpoint_v1(&stranger_ctx, &stranger_prompt, &opening, claim)
        .expect_err("the honest opening under a stranger's job");
    assert_eq!(
        palw_seat_s4_refusal_v1(refused),
        PalwSeatSegmentRefusalV1::NotTheClaims(misaka_palw_base0::segment_opening::Base0SegmentRefusalV1::NotTheSeatsJob.to_string()),
        "a refusal that drops the honest bytes — which is why no unauthenticated job is resumed under"
    );
}

/// **F1: whose an `Err` is.** Every link of the opening's authentication, as the family names it,
/// is the opening's (`NotTheClaims`); a replay that failed after them, a task that did not finish,
/// and an error this build does not know are this host's (`Local`). The match is exhaustive, so a
/// link the family adds does not compile here until it is sorted.
#[test]
fn f1_an_err_is_the_openings_only_when_the_family_names_an_authentication_link() {
    use misaka_palw_base0::segment_opening::Base0SegmentRefusalV1 as R;
    let is_a_link = |refusal: &R| match refusal {
        R::NotAnOpening
        | R::BindingDoesNotVerify
        | R::NotTheClaimsExecution
        | R::NotTheClaimsTrace
        | R::NotTheSeatsJob
        | R::NotThisClass
        | R::PriceIsNotTheGeometrys
        | R::NotTheSeatsSegment
        | R::NotTheSegmentsRange
        | R::ProofNotTheSegments
        | R::ProofPathNotTheRanges
        | R::NotMainStepLeaves
        | R::AnchorNotCommitted
        | R::AnchorNotCanonical
        | R::AnchorCarriesNoState
        | R::AnchorNotAResumePoint
        | R::AnchorPastTheRange
        | R::SeedPinMissing
        | R::SeedPinNotCommitted
        | R::SeedPinUnexpected
        | R::PromptNotTheJobs => true,
        R::Replay(_) => false,
    };
    let mut seen = HashSet::new();
    for link in PALW_SEAT_S4_AUTHENTICATION_REFUSALS_V1.iter() {
        assert!(is_a_link(link), "{link:?}");
        assert!(seen.insert(link.to_string()), "{link:?} named twice");
        assert_eq!(palw_seat_s4_refusal_v1(link.to_string()), PalwSeatSegmentRefusalV1::NotTheClaims(link.to_string()));
    }
    assert_eq!(seen.len(), 21, "every link listed");
    for local in [
        R::Replay("a kernel error".into()).to_string(),
        "the replay task did not finish: task panicked".to_string(),
        "a family this build does not know".to_string(),
    ] {
        assert_eq!(palw_seat_s4_refusal_v1(local.clone()), PalwSeatSegmentRefusalV1::Local(local));
    }
    let route = body_of(source(), "    async fn seat_s4_resume_v1(");
    assert!(route.contains(".map_err(palw_seat_s4_refusal_v1)"), "the route's task sorts its Err");
}

/// **F1: this host's failure is never the producer's.** A segment task that panics, or whose
/// kernel fails after the opening authenticated, keeps its bytes in the pool, counts the seat as
/// served, and is started again within the bound: an honest opening whose first run failed here
/// licenses on its second, and one that always fails spends the bound and leaves the seat silent
/// — `Starved { served: true }`, never the material wait's `served: false`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn f1_a_local_failure_is_never_unserved_and_is_retried_within_the_bound() {
    let (claim, _segment, request) = partial_claim(100);
    let (mut resumes, mut replays) = (PalwSeatResumesV1::default(), PalwSeatReplaysV1::default());
    let mut openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
    openings.insert((claim.claim, request), vec![b"real".to_vec()]);
    let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let flaky = |replays: &mut PalwSeatReplaysV1, key, _segment, _opening: Vec<u8>| {
        let runs = runs.clone();
        replays.start_segment(key, h(0x2E), true, 0, None, Box::new(floor_backend()), move |_| {
            if runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                panic!("this host's first run does not finish");
            }
            Ok(true)
        });
        true
    };
    assert_eq!(
        settle_with(&mut resumes, &claim, &mut openings, &mut replays, flaky).await,
        PalwSeatResumeStepV1::Licensed(claim.mask),
        "the same bytes, started again, license"
    );
    assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 2);
    // A panicked task reads as this host's failure.
    let mut slots = PalwSeatReplaysV1::default();
    let key = (h(0xC9), h(0x10B));
    slots.start_segment(key, h(0x2E), true, 0, None, Box::new(floor_backend()), |_| panic!("a kernel panic"));
    let polled = loop {
        match slots.poll_segment(&key, 1).await {
            PalwSeatSegmentPollV1::Running => tokio::time::sleep(std::time::Duration::from_millis(5)).await,
            other => break other,
        }
    };
    assert!(matches!(polled, PalwSeatSegmentPollV1::Done { result: Err(PalwSeatSegmentRefusalV1::Local(_)), fresh: true }));
    slots.forget_segment(&key);
    assert_eq!(slots.poll_segment(&key, 2).await, PalwSeatSegmentPollV1::Absent, "forgotten: startable again");
    // A kernel that always fails here: the bound, then silence — never unserved.
    let other = PalwSeatResumeClaimV1 { claim: h(0xC3), ..claim };
    openings.insert((other.claim, request), vec![b"real".to_vec()]);
    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failing = |replays: &mut PalwSeatReplaysV1, key, _segment, _opening: Vec<u8>| {
        let failures = failures.clone();
        replays.start_segment(key, h(0x2E), true, 0, None, Box::new(floor_backend()), move |_| {
            failures.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(palw_seat_s4_refusal_v1(misaka_palw_base0::segment_opening::Base0SegmentRefusalV1::Replay("oom".into()).to_string()))
        });
        true
    };
    assert_eq!(
        settle_with(&mut resumes, &other, &mut openings, &mut replays, failing).await,
        PalwSeatResumeStepV1::Starved { missing: vec![], served: true },
        "spent on this host's failures: silent, and never the material wait's unserved"
    );
    assert_eq!(failures.load(std::sync::atomic::Ordering::SeqCst), PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1 as usize, "bounded");
    assert_eq!(openings.get(&(other.claim, request)).map(Vec::len), Some(1), "the bytes were kept, never dropped");
}

/// **F2: a full seat whose own replay did not run accuses nobody.** A replay this host's timing of
/// the class says returns after the deadline (`fits`), and one refused twice, come to `NoVerdict`
/// and the full seat reaches the tail: past SEAT-R the attempt lane is silent there, and so is a
/// free-prompt seat holding a job of the claim's — at any DAA, with a licence standing or not. A
/// free-prompt seat served no job waits and is noted; a refuted claim keeps its path; below
/// SEAT-R the wait runs as it always did.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f2_a_full_seat_whose_replay_did_not_run_never_reaches_unavailable() {
        use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    use kaspa_consensus_core::palw_backend::PalwReplayRootsV1;
    let class = h(0x2E);
    let mut replays = PalwSeatReplaysV1::default();
    async fn returned(replays: &mut PalwSeatReplaysV1, key: &(Hash64, Hash64), now: u64) -> PalwSeatReplayPollV1 {
        loop {
            match replays.poll(key, now).await {
                PalwSeatReplayPollV1::Running => tokio::time::sleep(std::time::Duration::from_millis(5)).await,
                other => return other,
            }
        }
    }
    // Not in time: the class's last replay here took 700 DAA, and a claim bound at 100 closes at 600.
    let timed = (h(0xC1), h(0x10B));
    replays.start(timed, class, false, 0, None, Box::new(floor_backend()), |_| {
        Ok(PalwReplayRootsV1 { execution_root: h(1), trace_root: h(2), work_leaves: None, output_root: None })
    });
    assert!(matches!(returned(&mut replays, &timed, 700).await, PalwSeatReplayPollV1::Done { result: Ok(_), .. }));
    assert!(!replays.fits(&class, 100, 600), "not started: it would return after the deadline");
    // Refused twice: the retry is spent and the step is no verdict.
    let refused = (h(0xC2), h(0x10B));
    for attempt in 0..2 {
        replays.start(refused, class, false, 100, None, Box::new(floor_backend()), |_| Err("the kernel refused".into()));
        let PalwSeatReplayPollV1::Done { result, .. } = returned(&mut replays, &refused, 110).await else { panic!("returned") };
        assert_eq!(palw_seat_replay_step_v1(&result, h(1), h(2), 0, h(3)), PalwSeatReplayStepV1::NoVerdict);
        assert_eq!(replays.retry_refused(&refused, 110, 600), attempt == 0, "one retry, then the refusal stands");
    }
    // The tail: silent for the attempt lane (the fp job is not even asked), and for a free-prompt
    // seat holding a job of the claim's.
    let unasked = || -> bool { panic!("the attempt lane holds its job by the anchor") };
    assert_eq!(palw_seat_tail_v1(true, false, false, unasked), PalwSeatTailV1::Silent);
    assert_eq!(palw_seat_tail_v1(true, true, false, || true), PalwSeatTailV1::Silent);
    assert_eq!(palw_seat_tail_v1(true, true, false, || false), PalwSeatTailV1::Waits { unserved: true }, "served no job");
    for free_prompt in [false, true] {
        let unasked = || -> bool { panic!("not asked") };
        assert_eq!(palw_seat_tail_v1(true, free_prompt, true, unasked), PalwSeatTailV1::Waits { unserved: false }, "refuted");
        let unasked = || -> bool { panic!("not asked") };
        assert_eq!(palw_seat_tail_v1(false, free_prompt, false, unasked), PalwSeatTailV1::Waits { unserved: false }, "below");
    }
    // Where it is read: after the pull, before the one tail wait and its note — and `Silent` ends
    // the verdict with nothing filed.
    let source = source();
    let block = &source[source.find_code("let verdict = 'verdict: {").unwrap()..];
    let block = &block[..block.find_code("let Some(verdict) = verdict else { continue };").unwrap()];
    let gate = block.find_code("match palw_seat_tail_v1(seat_r, duty.free_prompt, refuted_here, || {").expect("the tail's gate");
    let wait = block.rfind("palw_seat_material_wait_ends_v1(").expect("the tail's wait");
    assert!(block[..gate].rfind("self.request_material_signed(").is_some(), "the pull still runs");
    let silent = "PalwSeatTailV1::Silent => {\n                            service.note_served(duty.claim_id);\n                            break 'verdict None;";
    assert!(gate < wait && block[gate..wait].contains_code(silent), "Silent: served, and nothing filed");
    let note = "PalwSeatTailV1::Waits { unserved: true } => service.note_unserved(duty.claim_id, current_daa),";
    assert!(block[gate..wait].contains(note), "noted only when served no job");
    assert!(!block.contains_code("if seat_r && !refuted {\n                        unserved.entry("), "the old unconditional note is gone");
}

/// **F4: a resuming partial seat is not held to a whole replay's memory.** The duty loop's
/// pre-check prices a full seat at the class's job; a `PartialResumes` duty skips it, because the
/// SEAT-S4 route reserves the partial seat's streamed need a segment at a time.
#[test]
fn f4_the_pre_check_does_not_price_a_resuming_partial_seat_as_a_full_seat() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let source = source();
    let block = &source[source.find_code("for index in order {").expect("the duty loop")..];
    let check = block.find_code("self.replay_memory_budget_v1(&session, Some((duty.class_id, duty.artifact_root)))").expect("the check");
    let guard = &block[..check];
    let guard = &guard[guard.rfind("if !(seat_r").expect("its guard")..];
    let squeezed: String = guard.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        squeezed
            .contains_code("if!(seat_r&&(seat_replays.holds_claim(&duty.claim_id)||seat_r_duty.role==PalwSeatRRoleV1::PartialResumes))"),
        "past SEAT-R only: below it the check prices every duty as it did"
    );
    let route = body_of(source, "    async fn seat_s4_resume_v1(");
    assert!(
        route.contains_code("self.reserve_replay_at_widest_run_v1(\"partial-seat\", backend.as_mut(), duty.class_id, duty.claim_id"),
        "its own reservation"
    );
}

/// **F5: a claim first seen while its class is unread gets `R_eff` once the class reads.** Noted
/// at its own retention meanwhile and read again each tick; the long-D row's horizon replaces it
/// once the registry answers; and a claim read under a read class is never read again.
#[test]
fn f5_a_long_d_claim_first_seen_unread_gets_r_eff_once_its_class_reads() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let (mut liabilities, mut unread) = (HashMap::new(), HashSet::new());
    let claim = h(0x51);
    let horizon = |spans: Option<u32>| Some(if spans.is_some_and(|s| s > 100) { 40_000 } else { 6_400 });
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, claim, PalwSeatClassReadV1::Unread, horizon);
    assert_eq!((liabilities.get(&claim), unread.contains(&claim)), (Some(&6_400), true), "its own retention meanwhile");
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, claim, PalwSeatClassReadV1::Unread, horizon);
    assert!(unread.contains(&claim), "still unread: read again");
    let long_d = PalwSeatClassReadV1::Rowed { verification_window_spans: 2_799, held_to_final: true, bought: false };
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, claim, long_d, horizon);
    assert_eq!((liabilities.get(&claim), unread.contains(&claim)), (Some(&40_000), false), "R_eff, once read");
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, claim, long_d, |_| panic!("read once"));
    // Unrowed is a read: final at once. No record yet notes nothing, and asks again.
    let (unrowed, recordless) = (h(0x52), h(0x53));
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, unrowed, PalwSeatClassReadV1::Unrowed, horizon);
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, unrowed, PalwSeatClassReadV1::Unrowed, |_| panic!("once"));
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, recordless, long_d, |_| None);
    assert!(!liabilities.contains_key(&recordless));
    palw_seat_note_duty_liability_v1(&mut liabilities, &mut unread, recordless, long_d, horizon);
    assert_eq!(liabilities.get(&recordless), Some(&40_000));
    // The sweep keeps the unread set inside the liabilities.
    let sweep = &source()[source().find_code("// R2: past SEAT-R also every claim this seat held a duty on").expect("the sweep")..];
    assert!(sweep.contains_code("liabilities_unread.retain(|claim| retention_liabilities.contains_key(claim));"));
}

/// **F6: the asks' pacing leaves with the claim.** Past SEAT-R `requested_intervals` is swept with
/// the served openings, to the claims a duty, court or dispute still names; below it, as it was.
#[test]
fn f6_requested_intervals_is_swept_past_seat_r() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let source = source();
    let sweep = source
        .find_code("if seat_r {\n                pool_books.intervals.resume_mut().retain(|(claim, _), _| live.contains(claim));")
        .expect("H1's sweep");
    let rest = &source[sweep..];
    let rest = &rest[..rest.find("\n        }\n").expect("its end")];
    assert!(rest.contains_code("requested_intervals.retain(|(claim, _), _| live.contains(claim));"));
    assert_eq!(source.matches("requested_intervals.retain(").count(), 1, "past the fence only");
}

/// **N-5: the material wait** — `min(W_r/2, 60)` past SEAT-R, half the window below it; at once
/// past it when a licence stands (asked only while the time has not come); and never while the
/// seat replays: every replay and resume `Waiting` ends the verdict with nothing filed, before any
/// wait is read.
#[test]
fn n5_the_material_wait_is_capped_past_seat_r_and_never_fires_while_replaying() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    assert_eq!(PALW_SEAT_MATERIAL_WAIT_CAP_DAA_V1, 60);
    assert_eq!(palw_seat_material_wait_until_v1(true, 1_000, 1_600), 1_060, "the floor: min(300, 60)");
    assert_eq!(palw_seat_material_wait_until_v1(true, 1_000, 3_799), 1_060, "the 2M row: min(1,399, 60)");
    assert_eq!(palw_seat_material_wait_until_v1(true, 1_000, 1_040), 1_020, "a devnet window: its half is shorter");
    assert_eq!(palw_seat_material_wait_until_v1(false, 1_000, 1_600), 1_300, "below SEAT-R: half the window, as always");
    let never = || -> bool { panic!("asked only while the time has not come") };
    assert!(palw_seat_material_wait_ends_v1(true, 1_000, 1_600, 1_060, never));
    assert!(!palw_seat_material_wait_ends_v1(true, 1_000, 1_600, 1_059, || false));
    assert!(palw_seat_material_wait_ends_v1(true, 1_000, 1_600, 1_001, || true), "a licence stands: at once");
    assert!(!palw_seat_material_wait_ends_v1(false, 1_000, 1_600, 1_001, || true), "below SEAT-R the licence is not read");
    // Never while replaying: in the verdict block every wait is read after the replay's and the
    // resume's `Waiting` arms, which end the verdict with nothing filed.
    let source = source();
    let block = &source[source.find_code("let verdict = 'verdict: {").unwrap()..];
    let block = &block[..block.find_code("let Some(verdict) = verdict else { continue };").unwrap()];
    let waits: Vec<usize> = block.match_indices("palw_seat_material_wait_ends_v1(").map(|(i, _)| i).collect();
    assert_eq!(
        waits.len(),
        5,
        "the tensor claim's pass (RFC-0003), the FP resume, the FP replaying partial seat, the attempt resume, and the tail"
    );
    for (n, at) in waits.iter().enumerate() {
        let before = &block[..*at];
        let resume_wait = before.rfind("PalwSeatResumeStepV1::Waiting => break 'verdict None,");
        let replay_wait = before.rfind("(PalwSeatReplayStepV1::Waiting, _) => break 'verdict None,");
        // The tensor claim's pass (RFC-0003) matches on its one step alone, not on a `(step, bytes)` pair.
        let tensor_wait = before.rfind_code("PalwSeatReplayStepV1::Waiting => break 'verdict None,");
        assert!(resume_wait.or(replay_wait).or(tensor_wait).is_some(), "wait {n}: a Waiting arm precedes it");
    }
    assert_eq!(block.matches("PalwSeatResumeStepV1::Waiting => break 'verdict None,").count(), 2);
    // A resuming seat that was served — an opening authenticated as the claim's, or on the FP lane
    // a job of the claim's held — never reaches a wait: it abstains.
    assert_eq!(block.matches("PalwSeatResumeStepV1::Starved { served: true, .. } => break 'verdict None,").count(), 2);
    for at in block.match_indices("PalwSeatResumeStepV1::Starved { served: false, .. } => {").map(|(i, _)| i) {
        let served = block[..at].rfind("PalwSeatResumeStepV1::Starved { served: true, .. } => break 'verdict None,").expect("before");
        assert!(at - served < 200, "the served arm is the unserved one's neighbour");
    }
    assert_eq!(block.matches("(PalwSeatReplayStepV1::Waiting, _) => break 'verdict None,").count(), 2);
    // And the licence landing on an unserved claim is P2-6's seam.
    assert!(palw_seat_unserved_licence_v1(Some(&PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 9 })));
    assert!(palw_seat_unserved_licence_v1(Some(&PalwClaimPhaseV2::Final { final_daa: 9 })));
    assert!(!palw_seat_unserved_licence_v1(Some(&PalwClaimPhaseV2::PanelBound { bound_daa: 9 })));
    assert!(!palw_seat_unserved_licence_v1(None));
}

/// **N-5 while a segment resume runs: `Waiting`, never `Starved`,** however long past the wait.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n5_a_resuming_seat_is_never_starved_while_its_replay_runs() {
    let (claim, _segment, request) = partial_claim(100);
    let (mut resumes, mut replays) = (PalwSeatResumesV1::default(), PalwSeatReplaysV1::default());
    let mut openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
    openings.insert((claim.claim, request), vec![b"real".to_vec()]);
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let start = |replays: &mut PalwSeatReplaysV1, key, _segment, _opening: Vec<u8>| {
        let gate = gate.clone();
        replays.start_segment(key, h(0x2E), true, 0, None, Box::new(floor_backend()), move |_| {
            gate.lock().unwrap().recv().map_err(|e| PalwSeatSegmentRefusalV1::Local(e.to_string()))?;
            Ok(true)
        });
        true
    };
    assert_eq!(resumes.step(&claim, &mut openings, &mut replays, start).await, PalwSeatResumeStepV1::Waiting);
    let late = PalwSeatResumeClaimV1 { now_daa: 100 + 10 * PALW_SEAT_MATERIAL_WAIT_CAP_DAA_V1, ..claim };
    assert_eq!(
        resumes.step(&late, &mut openings, &mut replays, |_: &mut PalwSeatReplaysV1, _, _, _| panic!("in flight")).await,
        PalwSeatResumeStepV1::Waiting,
        "ten X_ASKs on, still replaying: Waiting"
    );
    openings.clear();
    assert_eq!(
        resumes.step(&late, &mut openings, &mut replays, |_: &mut PalwSeatReplaysV1, _, _, _| panic!("in flight")).await,
        PalwSeatResumeStepV1::Waiting,
        "the candidate in flight is polled whatever the pool holds: never read as unserved"
    );
    release.send(()).unwrap();
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    assert_eq!(settle(&mut resumes, &late, &mut openings, &mut replays, &starts).await, PalwSeatResumeStepV1::Licensed(claim.mask));
}

/// **R2: the foreign retention keeps a live-liability claim's files** — a claim whose duty ended
/// (licensed) and whose retention horizon has not passed stays whatever its files' age; one past
/// its horizon, and one never this seat's, go by age as before; the liabilities are bounded, the
/// soonest horizon out first; and a long-D class's horizon reaches past its own retention.
#[test]
fn r2_the_prune_keeps_a_live_liability_claims_files() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let dir = std::env::temp_dir().join(format!("seat-s-r2-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (liable, expired, stranger) = (h(0x11), h(0x22), h(0x33));
    for claim in [liable, expired, stranger] {
        std::fs::write(dir.join(format!("{claim}.material")), b"bytes").unwrap();
    }
    let mut liabilities = HashMap::new();
    palw_seat_note_liability_v1(&mut liabilities, liable, 9_000);
    palw_seat_note_liability_v1(&mut liabilities, expired, 4_999);
    let live: HashSet<Hash64> = HashSet::new();
    let pins = palw_seat_retention_pins_v1(&live, &liabilities, 5_000);
    assert!(pins.contains(&liable) && !pins.contains(&expired));
    let hours = |n: u64| std::time::Duration::from_secs(n * 3600);
    prune_foreign_retention_v1(&dir, &pins, std::time::SystemTime::now() + hours(100), 4_096);
    assert!(dir.join(format!("{liable}.material")).exists(), "a live liability's file stays past the age bound");
    assert!(!dir.join(format!("{expired}.material")).exists() && !dir.join(format!("{stranger}.material")).exists());
    let _ = std::fs::remove_dir_all(&dir);
    // Bounded, the soonest horizon out first.
    let mut bounded = HashMap::new();
    for n in 0..PALW_SEAT_RETENTION_LIABILITIES_MAX_V1 as u64 {
        palw_seat_note_liability_v1(&mut bounded, h(0x1000 + n), 10_000 + n);
    }
    palw_seat_note_liability_v1(&mut bounded, h(0x9999), 50_000);
    assert_eq!(bounded.len(), PALW_SEAT_RETENTION_LIABILITIES_MAX_V1);
    assert!(!bounded.contains_key(&h(0x1000)) && bounded.contains_key(&h(0x9999)));
    // The horizon: the claim's own retention, and a long-D class's longer one.
    let chain = crate::palw_receipt_pool::tests::Chain::new();
    let mut claim = chain.state.claim(&chain.claim).expect("the fixture's claim").clone();
    claim.accepted_daa = 1_000;
    claim.trace_retention_daa = 6_400;
    let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let lifecycle_row = super::seat_r_tests::lifecycle_row;
    assert_eq!(
        palw_seat_retention_horizon_v1(&params, &claim, 1_010, Some(&lifecycle_row(3))),
        6_400,
        "the 8k row: its own retention"
    );
    assert_eq!(palw_seat_retention_horizon_v1(&params, &claim, 1_010, None), 6_400, "unrowed: its own retention");
    // Long-D: the chain's `W_r(c)` (13,995 for the 2M row) and its challenge window in force (120 on
    // testnet-12 from genesis, not the bundle's long 1,200).
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("testnet-12 is ConsensusV2")
    };
    let sp = &bundle.state;
    assert_eq!(sp.window_challenge_at(1_010), 120);
    assert_eq!(
        palw_seat_retention_horizon_v1(&params, &claim, 1_010, Some(&lifecycle_row(2_799))),
        1_000
            + 2 * (sp.window_bind() + 13_995)
            + 120
            + sp.window_court()
            + kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(sp),
        "long-D: R_eff"
    );
    // 25 spans derive D = 125, past 120 (long-D); 24 derive 120, at it (not).
    assert!(palw_seat_retention_horizon_v1(&params, &claim, 1_010, Some(&lifecycle_row(25))) > 6_400, "D 125: long-D");
    assert_eq!(palw_seat_retention_horizon_v1(&params, &claim, 1_010, Some(&lifecycle_row(24))), 6_400, "D 120: not long-D");
    // Past SEAT-R the pins are the liabilities' too; below it, the live claims as always.
    let sweep = &source()[source().find_code("// R2: past SEAT-R also every claim this seat held a duty on").expect("the sweep")..];
    assert!(
        sweep.contains_code("if seat_r {")
            && sweep.contains_code("} else {\n                *self.foreign_pinned.lock().unwrap() = live.clone();")
    );
}

/// **Below SEAT-R every path is what it was**: the arms, the forms whatever the role, the material
/// wait, and every SEAT-S route behind the fence — the S4 route, the N-5 reads, R2's pins and the
/// deadline display.
#[test]
fn below_seat_r_the_seat_s_paths_are_what_they_were() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let mask = PalwSegmentMaskV2::single(1);
    for arm in PalwSeatArmV1::ALL {
        assert!(palw_seat_arm_licenses_v1(false, arm), "{arm:?}");
    }
    for role in
        [PalwSeatRRoleV1::FullSeat, PalwSeatRRoleV1::PartialReplays, PalwSeatRRoleV1::PartialResumes, PalwSeatRRoleV1::PartialAbstains]
    {
        assert_eq!(
            palw_seat_receipt_forms_v1(true, false, true, role, mask),
            PalwSeatReceiptFormsV1 { segments: Some(mask), whole: false }
        );
        assert_eq!(palw_seat_receipt_forms_v1(false, false, true, role, mask), PalwSeatReceiptFormsV1 { segments: None, whole: true });
    }
    for window in [40u64, 600, 2_799] {
        assert_eq!(palw_seat_material_wait_until_v1(false, 500, 500 + window), 500 + window / 2);
    }
    let source = source();
    let block = &source[source.find_code("let verdict = 'verdict: {").unwrap()..];
    let block = &block[..block.find_code("let Some(verdict) = verdict else { continue };").unwrap()];
    let squeezed: String = block.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squeezed.contains_code("ifseat_r&&seat_r_duty.role==PalwSeatRRoleV1::PartialResumes{"), "the FP route: past the fence");
    let fence = block.find_code("let resolved = if seat_r {").unwrap();
    let old = fence + block[fence..].find_code("&& replayed.insert(seat_duty_panel_key_v1(duty))").unwrap();
    let attempt_route = block.rfind(".seat_s4_resume_v1(").unwrap();
    assert!(fence < attempt_route && attempt_route < old, "the attempt route: the fence's branch");
    let unasked = || -> bool { panic!("below SEAT-R the tail asks nothing") };
    assert_eq!(palw_seat_tail_v1(false, true, false, unasked), PalwSeatTailV1::Waits { unserved: false });
    assert!(block.contains_code("match palw_seat_tail_v1(seat_r, duty.free_prompt, refuted_here, || {"));
    assert!(
        source.contains_code("if seat_r {\n                pool_books.intervals.resume_mut().retain(|(claim, _), _| live.contains(claim));"),
        "H1's sweep"
    );
    let events = body_of(source, "    pub async fn worker(");
    assert!(events.contains_code("let per_claim = palw_seat_r_in_force_v1(&self.consensus_config.params, current_daa)"), "the display");
    let key = palw_seat_s4_candidate_key_v1(h(1), h(2), 5, 1, b"x");
    assert_ne!(key, palw_seat_s4_candidate_key_v1(h(1), h(2), 5, 2, b"x"), "a segment's own task");
    assert_ne!(key.1, h(2), "never a whole-job replay's key");
}
