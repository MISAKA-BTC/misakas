//! RFC-0002 row 14: the owning39 lifecycle frames an operator signs — retention release 71, expired consent-hold collection 72,
//! job collection 53, commit assembly 55 and the mining actions 73–79 — are the exact fixed extents the kernel route admits.
//! Frames assert nothing; the dormant folds decide. (Unbuilt test source; the consolidated validation phase runs it. Not a PASS.)
use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
use misaka_palw_sdk::native_payload_v29::*;

fn stage(p: &PreparedNativePayloadV29) -> (u8, Vec<u8>, usize) {
    match p.object() {
        K::StageExecutionV14 { action, body } => (*action, body.clone(), p.bytes().len()),
        other => panic!("owning39 frames are stage objects, got {other:?}"),
    }
}

#[test]
fn retention_release71_is_exact_kind_and_identity() {
    let id = [7u8; 64];
    for class in [false, true] {
        let (action, body, encoded) = stage(&prepare_retention_release_v39(class, id).unwrap());
        assert_eq!(action, 71);
        assert_eq!(body.len(), 67);
        assert_eq!(&body[..2], &39u16.to_le_bytes());
        assert_eq!(body[2], u8::from(class));
        assert_eq!(&body[3..], &id);
        assert_eq!(encoded, 67 + 7, "route extent 71 is exact");
    }
    assert!(prepare_retention_release_v39(true, [0; 64]).is_err());
    let mut raw = vec![1u8];
    raw.extend_from_slice(&id);
    assert_eq!(decode_native_payload_v39("source-release39", &raw, None).unwrap().bytes(), prepare_retention_release_v39(true, id).unwrap().bytes());
    raw[0] = 2;
    assert!(decode_native_payload_v39("source-release39", &raw, None).is_err());
    assert!(decode_native_payload_v39("source-release39", &raw[..64], None).is_err());
}

#[test]
fn consent_hold_reap72_is_the_exact_hold_id() {
    let id = [9u8; 64];
    let (action, body, encoded) = stage(&prepare_consent_hold_reap_v39(id).unwrap());
    assert_eq!((action, body.as_slice(), encoded), (72, id.as_slice(), 64 + 7));
    assert!(prepare_consent_hold_reap_v39([0; 64]).is_err());
    assert_eq!(decode_native_payload_v39("source-hold-reap39", &id, None).unwrap().bytes(), prepare_consent_hold_reap_v39(id).unwrap().bytes());
    assert!(decode_native_payload_v39("source-hold-reap39", &id[..63], None).is_err());
}

#[test]
fn collection53_and_assembly55_are_exact() {
    let (a, b, e) = stage(&prepare_collect_v39([3; 64]).unwrap());
    assert_eq!((a, b.len(), e), (53, 66, 66 + 7));
    let (a, b, e) = stage(&prepare_commit_assembly_v39([3; 64], [4; 64]).unwrap());
    assert_eq!((a, b.len(), e), (55, 130, 130 + 7));
    assert!(prepare_collect_v39([0; 64]).is_err());
    assert!(prepare_commit_assembly_v39([3; 64], [0; 64]).is_err());
}

#[test]
fn mining73_to_79_frames_match_route_extents() {
    use kaspa_consensus_core::palw_native_mining_v39::*;
    let (a, b, e) = stage(&prepare_mining_register_v39([1; 64], 5, 490).unwrap());
    assert_eq!((a, e), (REGISTER_MODEL_ACTION_V39, 76 + 7));
    assert_eq!(b.len(), 76);
    let (a, _, e) = stage(&prepare_mining_seat_v39([1; 64], 0).unwrap());
    assert_eq!((a, e), (MODEL_SEAT_ACTION_V39, 67 + 7));
    for kind in [1u8, 2] {
        let (a, _, e) = stage(&prepare_mining_seat_v39([1; 64], kind).unwrap());
        assert_eq!((a, e), (MODEL_RETIRE_ACTION_V39, 67 + 7));
    }
    let (a, _, e) = stage(&prepare_mining_commit_v39([1; 64], [2; 64], 0).unwrap());
    assert_eq!((a, e), (MINING_COMMIT_ACTION_V39, 138 + 7));
    for (action, kind) in [(ISSUE_RECEIPT_ACTION_V39, 0u8), (COLLECT_MINING_ACTION_V39, 0), (COLLECT_MINING_ACTION_V39, 1)] {
        let (a, _, e) = stage(&prepare_mining_subject_v39(action, kind, [5; 64]).unwrap());
        assert_eq!((a, e), (action, 67 + 7));
    }
    // 78 is not a stage frame: receipts are spent by the PNR9 header carriage.
    assert!(prepare_mining_subject_v39(SPEND_RECEIPT_ACTION_V39, 0, [5; 64]).is_err());
}
