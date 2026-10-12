//! **SEAT-S4's review, kept as a regression:** the liveness cells the committed tests do not cover —
//! a Qwen3.6 held v7 FOLD serving SC02 to a partial seat (the committed Qwen3.6 test opens a dense
//! capture), and the A16 per-call dense row (v2) resuming from committed chunks with a tiled pin.

mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1;
use kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::segment_opening::Base0SegmentOpeningV2;

fn sweep(label: &str, backend: &dyn PalwExecutionBackendV1, anchor: u64, seat_counts: &[u16]) -> (usize, usize) {
    let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(anchor)).expect("a job");
    let job = palw_attempt_job_v1(canonical, false);
    let out = backend.execute(&job, &prompt).expect("the attempt runs");
    let (mut genesis, mut resumed) = (0usize, 0usize);
    for &seats in seat_counts {
        for index in 0..palw_segment_count_v2(seats) {
            let bytes = backend
                .open_segment_checkpoint_v1(&out.material, seats, index)
                .unwrap_or_else(|e| panic!("{label} {seats}/{index}: {e}"));
            let opening = Base0SegmentOpeningV2::decode_v2(&bytes).expect("SC02");
            let claim = PalwSegmentClaimV1 {
                execution_root: out.execution_root,
                trace_root: out.trace_root,
                seat_count: seats,
                segment_index: index,
            };
            let replay = backend
                .replay_segment_from_checkpoint_v1(&job, &prompt, &bytes, claim)
                .unwrap_or_else(|e| panic!("{label} {seats}/{index}: refused an honest opening: {e}"));
            assert!(replay.matches, "{label} {seats}/{index}: honest opening does not match");
            if opening.anchor.is_some() {
                resumed += 1
            } else {
                genesis += 1
            }
            eprintln!(
                "{label} seats={seats} seg={index} align={} anchor={} pin={} bytes={}",
                opening.proof.align_level,
                opening.anchor.is_some(),
                opening.seed_pin.is_some(),
                bytes.len()
            );
            // A lie at the segment's first leaf: the liar's own opening never matches that segment.
            let lie = backend.execute_with_injected_fault(&job, &prompt, opening.leaf_start).expect("the drill lie");
            let lbytes = backend.open_segment_checkpoint_v1(&lie.material, seats, index).expect("the liar opens");
            let lclaim = PalwSegmentClaimV1 { execution_root: lie.execution_root, trace_root: lie.trace_root, ..claim };
            match backend.replay_segment_from_checkpoint_v1(&job, &prompt, &lbytes, lclaim) {
                Ok(r) => assert!(!r.matches, "{label} {seats}/{index}: a lie in the segment licensed"),
                Err(e) => eprintln!("{label} {seats}/{index}: the liar's opening refused: {e}"),
            }
        }
    }
    (genesis, resumed)
}

#[test]
fn qwen36_held_fold_serves_and_matches() {
    use kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7;
    use kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1;
    let (artifact, geometry) = qwen36_fixture();
    let held = qwen36_profile_v7(geometry).expect("the held graph-v7 projection");
    assert!(palw_attempt_capture_folds_v1(&held));
    let backend = qwen36_backend(&artifact, &held, (3, 4));
    let (genesis, resumed) = sweep("Qwen3.6 held v7 fold", &backend, 0x3E36_0001, &[2, 3, 5, 9]);
    assert!(genesis > 0 && resumed == 0, "a fold serves genesis only: {genesis}/{resumed}");
}

#[test]
fn a16_per_call_dense_resumes_from_committed_chunks() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2;
    let artifact = a16_artifact(128);
    let per_call = qwen25_a16_profile_v2(a16_geometry(128)).expect("the v2 row projects");
    let backend = a16_backend(&artifact, &per_call, (15, 6));
    let (genesis, resumed) = sweep("A16 v2 dense", &backend, 0xA16_0002, &[5, 9, 17, 33]);
    eprintln!("A16 v2 dense: genesis {genesis}, resumed {resumed}");
    assert!(resumed > 0, "some segment resumes from a committed checkpoint");
}
