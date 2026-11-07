#[test]
fn native_constraint_tail_graph_uses_exact_codec_quota_and_preserves_old_graph() {
    use crate::palw_native_resource_v1::{
        NativeDecodeErrorV1 as E, NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1,
    };
    let prefix =
        crate::palw_tir_public_claim_v1::palw_tir_public_prefix_history_frame_fixture_v1(false);
    assert!(prefix.frame.original_v1().job.tail.is_some());
    assert_eq!(native_constraint_graph_tail_requests_v3(&prefix.frame).unwrap(),0);
    // Structurally decoded version12 is NOT covered by exact version11 support.
    let original=prefix.frame.original_v1();let old=borsh::to_vec(original).unwrap();
    let mut changed=original.clone();changed.job.version=12;
    let new=borsh::to_vec(&changed).unwrap();assert_eq!(old.len(),new.len());
    let mut wire=borsh::to_vec(&prefix.frame).unwrap();
    let offsets=wire.windows(old.len()).enumerate().filter_map(|(i,b)|(b==old.as_slice()).then_some(i)).collect::<Vec<_>>();assert_eq!(offsets.len(),1);
    wire[offsets[0]..offsets[0]+old.len()].copy_from_slice(&new);
    let unsupported:PalwTirPublicClaimV1=borsh::from_slice(&wire).unwrap();
    assert!(matches!(native_constraint_graph_tail_requests_v3(&unsupported),Err(E::Resource(_))));
    let (fx, _, _) = crate::palw_tir_public_claim_v1::palw_tir_public_constraint_frame_fixture_v1();
    let frame = &fx.frame;
    let original = frame.original_v1();
    let profile = Hash64::from_u64_word(72);
    let source = PalwTirPublicSourceV1::new_native_v1(
        &fx.record,
        &fx.layout,
        frame.header_v1(),
        original.execution_root,
        original.trace_root,
        fx.rules.maximum_leaves,
    )
    .unwrap();
    let old = NativeSourceMaterialGraphV3::derive_v3(frame, &source, profile, B).unwrap();
    let set = TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
    let extent = graph_extent_from_transport_v3(
        set.signed_data_transport_upper().unwrap(),
        set.has_full_ids_stream_v3(),
    )
    .unwrap();
    let prelude = native_source_graph_prelude_v3(frame, &source, profile).unwrap();
    let total = prelude
        .checked_add(native_source_graph_output_v3(extent, set.len(), old.roles).unwrap())
        .unwrap();
    assert!(native_constraint_graph_tail_requests_v3(frame).unwrap() > 0);
    let raw = crate::palw_fp_constraint_job_v1::palw_fp_constraint_tail_v1(&original.job).unwrap();
    let mut malformed_wire = borsh::to_vec(frame).unwrap();
    let offsets = malformed_wire
        .windows(raw.len())
        .enumerate()
        .filter_map(|(i, b)| (b == raw.as_slice()).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(
        offsets.len(),
        1,
        "exact original constraint occurs once in actual frame"
    );
    // Keep a valid Borsh shape; start_frame becomes past the existing frame Vec.
    let start = offsets[0] + 2 + 64;
    malformed_wire[start..start + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    let malformed: PalwTirPublicClaimV1 = borsh::from_slice(&malformed_wire).unwrap();
    assert_eq!(
        native_constraint_graph_tail_requests_v3(&malformed).unwrap(),
        native_constraint_graph_tail_requests_v3(frame).unwrap()
    );
    assert!(matches!(
        native_check_constraint_graph_tail_v3(&malformed),
        Err(E::Decode(_))
    ));
    for f in [frame, &malformed] {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 {
            memory_bytes: prelude - 1,
            database_bytes: 0,
        });
        let result =
            NativeSourceMaterialGraphV3::derive_native_reserved_v3(f, &source, profile, B, |q| {
                checker.reserve_requests_v1(q, 0).map(Some)
            });
        assert!(matches!(result, Err(E::Resource(_))));
        assert_eq!(checker.outstanding(), (0, 0));
    }
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 {
        memory_bytes: total,
        database_bytes: 0,
    });
    let graph =
        NativeSourceMaterialGraphV3::derive_native_reserved_v3(frame, &source, profile, B, |q| {
            checker.reserve_requests_v1(q, 0).map(Some)
        })
        .unwrap();
    assert_eq!(graph.nodes, old.nodes);
    assert_eq!(graph.scope, old.scope);
    assert_eq!(graph.absolute_scope, old.absolute_scope);
    assert_eq!(graph.edges, old.edges);
    assert_eq!(checker.outstanding(), (total, 0));
    assert!(checker.reserve_requests_v1(1, 0).is_err());
    drop(graph);
    assert_eq!(checker.outstanding(), (0, 0));
    let result = NativeSourceMaterialGraphV3::derive_native_reserved_v3(
        &malformed,
        &source,
        profile,
        B,
        |q| checker.reserve_requests_v1(q, 0).map(Some),
    );
    assert!(matches!(result, Err(E::Decode(_))));
    assert_eq!(checker.outstanding(), (0, 0));
    let again =
        NativeSourceMaterialGraphV3::derive_native_reserved_v3(frame, &source, profile, B, |q| {
            checker.reserve_requests_v1(q, 0).map(Some)
        })
        .unwrap();
    drop(again);
    assert_eq!(checker.outstanding(), (0, 0));
}
