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
fn protected_chunk_target_binds_inner_actor_and_bond_prefix_deposits() {
    use kaspa_consensus_core::{
        Hash64,
        palw_kernel_route_v1::*,
        palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as O},
        tx::TransactionOutpoint,
    };
    use misaka_palw_kernel::{route::KernelRouteObjectV1 as K, rows::LedgerScalarsV1};
    let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(1), 1));
    let other = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(2), 2));
    let vector = Hash64::from_u64_word(3);
    let target = PalwKernelChunkTargetV1::FlatProofV6 { vector, actor: palw_kernel_bond_id_v1(&bond) };
    let encoded = borsh::to_vec(&target).unwrap();
    assert_eq!(encoded[0], 3);
    assert_eq!(borsh::from_slice::<PalwKernelChunkTargetV1>(&encoded).unwrap(), target);
    for n in 0..encoded.len() {
        assert!(borsh::from_slice::<PalwKernelChunkTargetV1>(&encoded[..n]).is_err());
    }
    let mut suffix = encoded;
    suffix.push(0);
    assert!(borsh::from_slice::<PalwKernelChunkTargetV1>(&suffix).is_err());
    let reveal = |signer, id| O::KernelRouteV1 {
        signer,
        signature: vec![1; 4],
        bytes: K::RevealFlatModelProofV6 { vector: id, salt: [1; 64], proof: vec![5, 0] }.encode(),
    };
    assert!(palw_kernel_chunk_inner_matches_target_v1(&reveal(bond, vector.as_bytes()), &target));
    assert!(!palw_kernel_chunk_inner_matches_target_v1(&reveal(other, vector.as_bytes()), &target));
    assert!(!palw_kernel_chunk_inner_matches_target_v1(&reveal(bond, [4; 64]), &target));
    let mut route = PalwKernelRouteStateV1::new(
        palw_kernel_route_policy_v1(Hash64::from_u64_word(4), Hash64::from_u64_word(5)),
        None,
        LedgerScalarsV1::default(),
    );
    for (owner, n, deposit) in [(bond, 1, 7), (bond, 2, 11), (other, 3, 100)] {
        let row = PalwKernelChunkGroupV1 {
            target,
            count: 2,
            opened_daa: 10,
            expires_daa: 20,
            deposit,
            parts: [(0, vec![3; 100_000])].into(),
        };
        route.aux.insert(
            (PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, palw_kernel_chunk_group_key_v1(&owner, &Hash64::from_u64_word(n))),
            borsh::to_vec(&row).unwrap(),
        );
    }
    assert_eq!(route.chunk_groups_of_v1(&bond), 2);
    assert_eq!(route.chunk_deposits_of_v1(&bond), 18);
    assert_eq!(route.chunk_groups_of_v1(&other), 1);
    assert_eq!(route.chunk_deposits_of_v1(&other), 100);
    let absent = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(2), 3));
    assert_eq!(route.chunk_deposits_of_v1(&absent), 0);
    assert!(route.expired_chunk_headers_v1(20).is_empty());
    assert_eq!(route.expired_chunk_headers_v1(21).len(), 3);
    assert_eq!(route.expired_chunk_headers_v1(21).iter().map(|(_, _, d)| d).sum::<u64>(), 118);
    let mut h = FlatModelVectorHeaderV3 {
        trace_root: [1; 64],
        class: vector,
        binding: vector,
        profile: [2; 64],
        scope_root: [3; 64],
        program_bytes_len: 100,
        pc_instances: 1,
        post_bytes_len: 100,
        max_filing_bytes: 1024,
        max_court_work: 10,
        max_court_scratch_bytes: 100,
        poster: bond,
        posted_daa: 10,
        liability_until: 110,
        reserved: 1,
        refuted: false,
    };
    let small = protected_flat_chunk_finish_work_v6(&h).unwrap();
    h.max_filing_bytes = 8192;
    assert!(protected_flat_chunk_finish_work_v6(&h).unwrap() > small);
    h.max_court_work = u64::MAX;
    assert!(protected_flat_chunk_finish_work_v6(&h).is_none());
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
        // 41–44 since the +5 renumbering at the `rfc02/merge-final` merge (route.rs: 36–40 belong to RFC02-T, G14R and RFC-7).
        assert!(matches!(o.tag(), 41..=44));
        assert_eq!(
            kaspa_consensus_core::palw_lifecycle_objects_v2::palw_kernel_route_inner_fence_v1(&o),
            Some(kaspa_consensus_core::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeV1)
        );
    }
    let tables =
        [57, 58, 59, 60, 61, 62, 63, FLAT_PROTECTED_TABLE_V6, FLAT_SEAL_TABLE_V6, FLAT_SEAL_ACTOR_TABLE_V6, FLAT_SEAL_COUNT_TABLE_V6];
    assert_eq!(tables.into_iter().collect::<std::collections::BTreeSet<_>>().len(), tables.len());
}
