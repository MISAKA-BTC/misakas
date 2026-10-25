//! Real Source geometry, bounded wire and original membership controls.
use super::super::tests::{geometry_with_trace, source};
use super::*;
use crate::palw_native_resource_v1::{NativeDemandLeaseV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
use crate::palw_step_leg::{step_merkle_range_siblings_v1, step_merkle_root_v1, step_opening_v1};
use crate::palw_tir_court_v1::{self as court, test_support::tiny_execution};
type Case = (PalwTirPublicSourceV1, PalwTirPublicRequestV1, PalwTirPublicResponseV1, Option<PalwTirPublicAuthenticatedIdsV1>);
fn cases() -> Vec<Case> {
    let x = tiny_execution(None);
    let src = source(&x);
    let level = height(src.leaf_count_v1());
    let d = court::build_tir_step_node_disclosure_v1(&x.binding, level, 0, &x, 1 << 40).unwrap();
    let count = src.leaf_count_v1().min(RUN as u64) as usize;
    let mut cases = vec![
        (
            src.clone(),
            PalwTirPublicRequestV1::StepNode { level, index: 0 },
            PalwTirPublicResponseV1::StepNode { frontier: d.frontier, siblings: d.siblings },
            None,
        ),
        (
            src.clone(),
            PalwTirPublicRequestV1::StepLeaf { index: 0 },
            PalwTirPublicResponseV1::StepLeaf { preimage: x.preimages[0].clone(), opening: step_opening_v1(&x.hashes, 0).unwrap() },
            None,
        ),
        (
            src,
            PalwTirPublicRequestV1::StepRun { first: 0, count: count as u32 },
            PalwTirPublicResponseV1::StepRun {
                preimages: x.preimages[..count].to_vec(),
                range: PalwStepRangeOpeningV1 {
                    first_leaf_index: 0,
                    leaf_hashes: x.hashes[..count].to_vec(),
                    siblings: step_merkle_range_siblings_v1(&x.hashes, 0, count).unwrap(),
                },
            },
            None,
        ),
    ];
    let vocab = TILE_LANES as u32 + 1;
    let (preliminary, _) = geometry_with_trace(vocab, 2, None);
    let mut tiles = Vec::new();
    let mut rows = Vec::new();
    for row in 0..2 {
        let hashes = vec![
            crate::palw_step_refute::tiled_logits_tile_leaf_v1(&preliminary.context_hash, row, 0, &vec![0; TILE_LANES]),
            crate::palw_step_refute::tiled_logits_tile_leaf_v1(&preliminary.context_hash, row, 1, &[0]),
        ];
        rows.push(step_merkle_root_v1(&hashes).unwrap());
        tiles.push(hashes);
    }
    let rows_root = step_merkle_root_v1(&rows).unwrap();
    let generated = vec![0, 0];
    let (src, _) = geometry_with_trace(vocab, 2, Some((rows_root, &generated)));
    assert_eq!(src.context_hash, preliminary.context_hash);
    let response = PalwTirPublicResponseV1::Ids { rows_root, ids: generated };
    let sealed =
        PalwTirPublicIdsAssemblerV1::default().push_v1(&src, &PalwTirPublicRequestV1::Ids { offset: 0 }, &response).unwrap().unwrap();
    let (frontier, siblings) = court::palw_tir_step_node_parts_v1(&rows, 1, 0).unwrap();
    cases.push((
        src.clone(),
        PalwTirPublicRequestV1::RowsNode { level: 1, index: 0 },
        PalwTirPublicResponseV1::RowsNode { frontier, siblings },
        Some(sealed.clone()),
    ));
    let row_opening = step_opening_v1(&rows, 0).unwrap();
    let (frontier, siblings) = court::palw_tir_step_node_parts_v1(&tiles[0], 1, 0).unwrap();
    cases.push((
        src.clone(),
        PalwTirPublicRequestV1::TileNode { row: 0, level: 1, index: 0 },
        PalwTirPublicResponseV1::TileNode { row_root: rows[0], row_opening: row_opening.clone(), frontier, siblings },
        Some(sealed.clone()),
    ));
    cases.push((
        src.clone(),
        PalwTirPublicRequestV1::TraceTile { row: 0, tile: 1 },
        PalwTirPublicResponseV1::TraceTile {
            row_root: rows[0],
            row_opening,
            lanes: vec![0],
            opening: step_opening_v1(&tiles[0], 1).unwrap(),
        },
        Some(sealed),
    ));
    cases.push((src, PalwTirPublicRequestV1::Ids { offset: 0 }, response, None));
    cases
}
fn child(key: &str, name: &str) -> bool {
    if std::env::var(key).as_deref() == Ok("1") {
        return true;
    }
    assert!(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
            .env(key, "1")
            .status()
            .unwrap()
            .success()
    );
    false
}
fn free(cap: u64) -> u64 {
    let (mut a, mut b) = (0, cap);
    while a < b {
        let n = a + (b - a).div_ceil(2);
        if reserve_native_requests_v1(n).is_ok() { a = n } else { b = n - 1 }
    }
    a
}
fn block(cap: u64, available: u64) -> Option<NativeDemandLeaseV1> {
    reserve_native_requests_v1(free(cap) - available).unwrap()
}
#[test]
fn seven_bounded_variants_match_raw_codec_and_original_native_authentication() {
    let cases = cases();
    assert_eq!(cases.len(), 7);
    for (src, request, response, ids) in cases {
        let bytes = borsh::to_vec(&response).unwrap();
        let raw = src.decode_response_v1(&request, &bytes).unwrap();
        let owned = src.decode_response_native_v2(&request, &bytes).unwrap();
        assert_eq!(*owned, raw);
        assert_eq!(scan_response(&bytes, src.maximum_leaf_bytes as usize).unwrap(), response_heap(&raw).unwrap());
        if matches!(request, PalwTirPublicRequestV1::Ids { .. }) {
            assert!(matches!(src.authenticate_response_owned_v2(&request, &owned, ids.as_ref()), Err(Error::Decode(_))));
        } else {
            let raw = src.authenticate_v1(&request, &response, ids.as_ref()).unwrap();
            let auth = src.authenticate_response_owned_v2(&request, &owned, ids.as_ref()).unwrap();
            assert_eq!(auth.source_id_v1(), raw.source_id_v1());
            assert_eq!(auth.request_v1(), raw.request_v1());
            assert_eq!(auth.response_v1(), raw.response_v1());
        }
    }
}
#[test]
fn malformed_and_truncated_wire_are_decode_faults_before_native_capacity() {
    if !child(
        "MISAKA_TIR_RESPONSE_MALFORMED_CHILD",
        &format!("{}::malformed_and_truncated_wire_are_decode_faults_before_native_capacity", module_path!()),
    ) {
        return;
    }
    let cases = cases();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    for (src, request, response, _) in &cases {
        let bytes = borsh::to_vec(response).unwrap();
        assert!(matches!(src.decode_response_native_v2(request, &bytes[..bytes.len() - 1]), Err(Error::Decode(_))));
        assert!(matches!(src.decode_response_native_v2(request, &bytes), Err(Error::Resource(_))));
    }
    for bytes in [vec![0, 255, 255, 255, 255], vec![2, 255, 255, 255, 255], vec![6, 0]] {
        assert!(matches!(scan_response(&bytes, LEAF_BYTES), Err(Error::Decode(_))));
    }
    assert!(matches!(io_fault(io::Error::new(io::ErrorKind::OutOfMemory, "opaque allocator diagnostic")), Error::Resource(_)));
    assert!(
        matches!(io_fault(io::Error::new(io::ErrorKind::InvalidData, "out of memory")), Error::Decode(_)),
        "classification uses actual kind, never text"
    );
}
#[test]
fn decoded_and_authenticated_owners_admit_actual_q_and_release_all_stage_scratch() {
    if !child(
        "MISAKA_TIR_RESPONSE_QUOTA_CHILD",
        &format!("{}::decoded_and_authenticated_owners_admit_actual_q_and_release_all_stage_scratch", module_path!()),
    ) {
        return;
    }
    let cases = cases();
    let cap = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: cap, database_bytes: 0 }));
    for (src, request, response, ids) in &cases {
        let bytes = borsh::to_vec(response).unwrap();
        let q = scan_response(&bytes, src.maximum_leaf_bytes as usize).unwrap();
        assert!(q > 0);
        let short = block(cap, q - 1);
        assert!(matches!(src.decode_response_native_v2(request, &bytes), Err(Error::Resource(_))));
        drop(short);
        assert_eq!(free(cap), cap);
        let exact = block(cap, q);
        let decoded = src.decode_response_native_v2(request, &bytes).unwrap();
        assert_eq!(free(cap), 0);
        drop(decoded);
        assert_eq!(free(cap), q);
        drop(exact);
        assert_eq!(free(cap), cap);
        if matches!(request, PalwTirPublicRequestV1::Ids { .. }) {
            continue;
        }
        let scratch = auth_scratch(response).unwrap();
        let q = add(response_heap(response).unwrap(), scratch).unwrap();
        let short = block(cap, q - 1);
        assert!(matches!(src.authenticate_response_owned_v2(request, response, ids.as_ref()), Err(Error::Resource(_))));
        drop(short);
        assert_eq!(free(cap), cap);
        let exact = block(cap, q);
        let auth = src.authenticate_response_owned_v2(request, response, ids.as_ref()).unwrap();
        assert_eq!(free(cap), scratch, "only cloned output remains; transient auth scratch is gone");
        drop(auth);
        assert_eq!(free(cap), q);
        drop(exact);
        assert_eq!(free(cap), cap);
    }
    let (src, request, mut response, _) = cases[1].clone();
    let PalwTirPublicResponseV1::StepLeaf { preimage, .. } = &mut response else { unreachable!() };
    preimage.value_count = preimage.value_count.wrapping_add(1);
    assert!(matches!(src.authenticate_response_owned_v2(&request, &response, None), Err(Error::Decode(_))));
    assert_eq!(free(cap), cap, "native semantic refusal releases output and workspace reservations");
}
fn complete_body(root: Hash64, ids: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in ids.chunks(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
        out.extend(borsh::to_vec(&PalwTirPublicResponseV1::Ids { rows_root: root, ids: chunk.to_vec() }).unwrap());
    }
    out
}
#[test]
fn complete_body_ids_match_canonical_assembler_and_reject_bad_chunks_and_eof() {
    let root = Hash64::from_u64_word(27);
    let generated = vec![0; 2050];
    let (src, _) = geometry_with_trace(8, 2050, Some((root, &generated)));
    let body = complete_body(root, &generated);
    let owned = src.decode_complete_ids_body_native_v2(&body).unwrap();
    assert_eq!(owned.ids_v1(), generated);
    assert_eq!(owned.rows_root_v1(), root);
    assert_eq!(owned.source_id_v1(), src.source_id_v1());
    let mut raw = PalwTirPublicIdsAssemblerV1::default();
    let mut sealed = None;
    for (ordinal, chunk) in generated.chunks(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize).enumerate() {
        sealed = raw
            .push_v1(
                &src,
                &PalwTirPublicRequestV1::Ids { offset: ordinal as u32 * PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 },
                &PalwTirPublicResponseV1::Ids { rows_root: root, ids: chunk.to_vec() },
            )
            .unwrap();
    }
    assert_eq!(owned.ids_v1(), sealed.unwrap().ids_v1());
    for end in [0, 1, 68, body.len() - 1] {
        assert!(matches!(src.decode_complete_ids_body_native_v2(&body[..end]), Err(Error::Decode(_))));
    }
    let mut bad = body.clone();
    bad[65..69].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(src.decode_complete_ids_body_native_v2(&bad), Err(Error::Decode(_))));
    let mut bad = body.clone();
    bad.push(0);
    assert!(matches!(src.decode_complete_ids_body_native_v2(&bad), Err(Error::Decode(_))));
    let mut bad = body.clone();
    bad[69] ^= 1;
    assert!(matches!(src.decode_complete_ids_body_native_v2(&bad), Err(Error::Decode(_))));
    let mut bad = body;
    bad[69 + 1024 * 4 + 1] ^= 1;
    assert!(matches!(src.decode_complete_ids_body_native_v2(&bad), Err(Error::Decode(_))));
}

#[test]
fn complete_id_output_and_existing_root_scratch_refuse_at_peak_q_minus_one_then_retry() {
    if !child(
        "MISAKA_TIR_IDS_BODY_QUOTA_CHILD",
        &format!("{}::complete_id_output_and_existing_root_scratch_refuse_at_peak_q_minus_one_then_retry", module_path!()),
    ) {
        return;
    }
    let root = Hash64::from_u64_word(27);
    let generated = vec![0; 2050];
    let (src, _) = geometry_with_trace(8, 2050, Some((root, &generated)));
    let body = complete_body(root, &generated);
    // Independent expected request of the existing classified complete-ID
    // validator: its ID-byte writer and two geometric context Writer buffers.
    // This is a typed conservative envelope, not exact allocator/RSS cost.
    let output = slots::<u32>(generated.len()).unwrap();
    let context_span = 2 + 4 + 11 * size_of::<Hash64>() + 32 + 3 * 4 + src.header.job_context.network_id.len();
    let context = context_span.max(512).checked_next_power_of_two().unwrap() as u64;
    let scratch = output + context * 2;
    let peak = output + scratch;
    let cap = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: cap, database_bytes: 0 }));
    super::super::complete_ids_owned_v2::reset_borrowed_observed_v2();
    let short = block(cap, peak - 1);
    assert!(matches!(src.decode_complete_ids_body_native_v2(&body), Err(Error::Resource(_))));
    assert_eq!(
        super::super::complete_ids_owned_v2::borrowed_observed_v2(),
        (1, 0),
        "destination was admitted; refusal is actual root-scratch reservation, not parser/earlier source stage"
    );
    drop(short);
    assert_eq!(free(cap), cap);
    super::super::complete_ids_owned_v2::reset_borrowed_observed_v2();
    let exact = block(cap, peak);
    let retained = src.decode_complete_ids_body_native_v2(&body).unwrap();
    assert_eq!(super::super::complete_ids_owned_v2::borrowed_observed_v2(), (1, 1));
    assert_eq!(retained.ids_v1(), generated);
    assert_eq!(free(cap), scratch, "retained ID output stays charged after root workspace retires");
    drop(retained);
    assert_eq!(free(cap), peak);
    drop(exact);
    assert_eq!(free(cap), cap);
    let mut wrong = body;
    wrong[69] ^= 1;
    assert!(matches!(src.decode_complete_ids_body_native_v2(&wrong), Err(Error::Decode(_))));
    assert_eq!(free(cap), cap);
}
