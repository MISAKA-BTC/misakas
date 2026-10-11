use kaspa_consensus_core::palw_flat_model_vector_v3::*;
use kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2;
fn fixture() -> FlatModelVectorPostV3 {
    use misaka_palw_kernel::{evidence::*, seg::*};
    FlatModelVectorPostV3 {
        version: 3,
        profile: misaka_palw_kernel::flat_court5::flat_trace_profile_id_v5(),
        scope_root: [11; 64],
        trace: ModelVectorPostV2 {
            version: 2,
            prompt: vec![0],
            max_new_tokens: 1,
            generated: vec![1],
            evidence: SegmentedEvidenceV2 {
                version: 2,
                header: EvidenceHeaderV1 {
                    network_domain: [1; 64],
                    ruleset_digest: [2; 64],
                    class_binding_id: [3; 64],
                    program_root: [4; 64],
                    artifact_root: [5; 64],
                    plan_root: [6; 64],
                },
                job_input_root: [7; 64],
                positions: 1,
                segment_len: SEG_LEN_V4,
                claim_root: [8; 64],
                suite: SuiteParamsV1::of(&misaka_palw_kernel::descriptor::k2_tir_v4_descriptor()),
            },
            segment_roots: vec![[9; 64]],
            implementation_roots: [[10; 64]; 3],
        },
    }
}
#[test]
fn protected_flat_v6_codec_identity_and_route_fences_are_separate_from_v3() {
    use kaspa_consensus_core::palw_flat_model_seal_v6::*;
    use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
    let p = FlatModelVectorPostV6 { version: 6, body: fixture() };
    let bytes = borsh::to_vec(&p).unwrap();
    assert_eq!(decode_flat_model_vector_post_v6(&bytes).unwrap(), p);
    assert!(decode_flat_model_vector_post_v3(&bytes).is_err());
    assert!(decode_flat_model_vector_post_v6(&borsh::to_vec(&p.body).unwrap()).is_err());
    assert_ne!(p.id([3; 64], [4; 64]), p.body.id([3; 64], [4; 64]));
    assert_ne!(p.id([3; 64], [4; 64]), p.id([4; 64], [4; 64]));
    for n in 0..bytes.len() {
        assert!(decode_flat_model_vector_post_v6(&bytes[..n]).is_err());
    }
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(decode_flat_model_vector_post_v6(&bad).is_err());
    let mut bad = bytes.clone();
    bad[134..138].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_flat_model_vector_post_v6(&bad).is_err());
    for o in [
        K::PostFlatModelVectorV6 { class: [3; 64], binding: [4; 64], post: bytes },
        K::SealFlatModelProofV6 { vector: [1; 64], seal: [2; 64] },
        K::RevealFlatModelProofV6 { vector: [1; 64], salt: [2; 64], proof: vec![5, 0] },
        K::ExpireFlatModelSealV6 { vector: [1; 64], actor: [2; 64] },
    ] {
        assert_eq!(K::decode(&o.encode()).unwrap(), o);
        assert!(matches!(o.tag(), 36..=39));
        assert_eq!(
            kaspa_consensus_core::palw_lifecycle_objects_v2::palw_kernel_route_inner_fence_v1(&o),
            Some(kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeV1)
        );
    }
    let tables =
        [57, 58, 59, 60, 61, 62, 63, FLAT_PROTECTED_TABLE_V6, FLAT_SEAL_TABLE_V6, FLAT_SEAL_ACTOR_TABLE_V6, FLAT_SEAL_COUNT_TABLE_V6];
    assert_eq!(tables.into_iter().collect::<std::collections::BTreeSet<_>>().len(), tables.len());
}
