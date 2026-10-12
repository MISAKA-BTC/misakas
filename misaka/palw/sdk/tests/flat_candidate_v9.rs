//! Synthetic metadata/grammar/signature checks, not checkpoint acceptance or Final.
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_route_v1::*,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::TransactionOutpoint,
};
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw_kernel::{descriptor::k2_tir_v4_descriptor, route::KernelRouteObjectV1 as K, trace::ParamCommitmentsV1};
use misaka_palw_sdk::flat_candidate_v9::*;

fn fixture() -> (misaka_palw_tir::TirProgramV1, ParamCommitmentsV1) {
    use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD};
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let scores = b.param("scores", DType::I32, &[65], false);
    let mut pre = b.block("pre", vec![]);
    let ids = pre.topk(scores, 0, 2);
    let values = pre.cast(ids, DType::I32);
    let pre = pre.finish(&[values]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    let out = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(out);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    let params = MapParams { tensors: [((0, None), Tensor::new(DType::I32, vec![65], (0..65).collect()).unwrap())].into() };
    (p, ParamCommitmentsV1::of_v3(&params))
}
fn prepare() -> PreparedFlatCandidateV9 {
    let (p, pc) = fixture();
    prepare_flat_candidate_v9([1; 64], k2_tir_v4_descriptor().digest(), &p.encode(), &pc, 128).unwrap()
}
fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(h(n), n as u32))
}

#[test]
fn candidate9_preparation_binds_generated_metadata_resource_and_explicit_new_namespace() {
    let prepared = prepare();
    assert_eq!(prepared.plan.version, 9);
    assert_eq!(&prepared.object.encode()[..2], &[1, 53]); // codex 48, renumbered at rfc02/merge-final
    assert_eq!(K::decode(&prepared.object.encode()).unwrap(), prepared.object);
    assert_eq!(prepared.class, prepared.record.id());
    let view = check_flat_candidate_record_v9(&prepared.record, &prepared.class).unwrap();
    assert_eq!(view.resources.root(), prepared.plan.resource_root);
    assert_eq!(view.scope.root(), prepared.plan.scope_root);
    assert_eq!(view.replay.root(), prepared.plan.replay_root);
    assert_eq!(borsh::to_vec(&view.scope).unwrap(), prepared.record.scope_bytes);
    assert_eq!(borsh::to_vec(&view.replay).unwrap(), prepared.record.replay_bytes);
    let (p, pc) = fixture();
    assert_ne!(
        prepared.class,
        prepare_flat_candidate_v9([2; 64], k2_tir_v4_descriptor().digest(), &p.encode(), &pc, 128).unwrap().class
    );
    assert_ne!(
        prepared.class,
        prepare_flat_candidate_v9([1; 64], k2_tir_v4_descriptor().digest(), &p.encode(), &pc, 127).unwrap().class
    );
    let v7 =
        misaka_palw_sdk::flat_candidate_v7::prepare_flat_candidate_v7([1; 64], k2_tir_v4_descriptor().digest(), &p.encode(), &pc, 128)
            .unwrap();
    assert_ne!(prepared.class, v7.class);
    assert_ne!(prepared.plan.scope_root, v7.plan.scope_root);
}
#[test]
fn candidate9_signing_independently_checks_all_roots_and_network_actor_signature() {
    let prepared = prepare();
    let key = ValidatorKey::from_seed([7; 32]);
    let signed = sign_flat_candidate_v9(&key, h(9), bond(3), &prepared.object).unwrap();
    let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature } = signed else { panic!() };
    assert_eq!(signer, bond(3));
    assert_eq!(K::decode(&bytes).unwrap(), prepared.object);
    assert!(key.verify_with_context(
        palw_kernel_route_message_v1(h(9), &signer, &bytes).as_byte_slice(),
        &signature,
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1
    ));
    assert!(!key.verify_with_context(
        palw_kernel_route_message_v1(h(8), &signer, &bytes).as_byte_slice(),
        &signature,
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1
    ));
    for field in 0..3 {
        let mut wrong = prepared.object.clone();
        let K::RegisterFlatModelCandidateV9 { scope_root, replay_root, resource_root, .. } = &mut wrong else { panic!() };
        match field {
            0 => scope_root[0] ^= 1,
            1 => replay_root[0] ^= 1,
            _ => resource_root[0] ^= 1,
        }
        assert!(sign_flat_candidate_v9(&key, h(9), bond(3), &wrong).is_err());
    }
    assert!(misaka_palw_sdk::flat_candidate_v7::sign_flat_candidate_v7(&key, h(9), bond(3), &prepared.object).is_err());
}
#[test]
fn bounded_record_codec_refuses_nested_count_namespace_layer_truncation_and_trailing_floods() {
    let prepared = prepare();
    let bytes = encode_flat_candidate_record_v9(&prepared.record, &prepared.class).unwrap();
    assert_eq!(decode_flat_candidate_record_v9(&bytes, &prepared.class).unwrap(), prepared.record);
    assert!(decode_flat_candidate_record_v9(&bytes, &[0; 64]).is_err());
    for n in 0..bytes.len() {
        assert!(decode_flat_candidate_record_v9(&bytes[..n], &prepared.class).is_err(), "prefix {n}");
    }
    let program_count = 64 + borsh::to_vec(&prepared.plan).unwrap().len();
    let pc_count = program_count + 4 + prepared.record.program_bytes.len();
    let pc_row = pc_count + 4;
    let scope_count = pc_row + 67;
    let replay_count = scope_count + 4 + prepared.record.scope_bytes.len();
    for offset in [program_count, pc_count, scope_count, replay_count] {
        let mut wrong = bytes.clone();
        wrong[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_flat_candidate_record_v9(&wrong, &prepared.class).is_err());
    }
    let mut wrong = bytes.clone();
    wrong[pc_row..pc_row + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(decode_flat_candidate_record_v9(&wrong, &prepared.class).is_err());
    let mut wrong = bytes;
    wrong.push(0);
    assert!(decode_flat_candidate_record_v9(&wrong, &prepared.class).is_err());
}
#[test]
fn stored_class_consistency_is_insufficient_without_complete_independent_geometry_and_tariff_checks() {
    let prepared = prepare();
    for field in 0..9 {
        let mut wrong = prepared.record.clone();
        match field {
            0 => wrong.plan.resource_root[0] ^= 1,
            1 => wrong.plan.scope_root[0] ^= 1,
            2 => wrong.plan.replay_root[0] ^= 1,
            3 => wrong.plan.native_restore_work -= 1,
            4 => wrong.plan.scope_metadata_work -= 1,
            5 => wrong.plan.scope_bytes_len += 1,
            6 => wrong.scope_bytes[0] ^= 1,
            7 => wrong.replay_bytes[0] ^= 1,
            _ => wrong.param_commitments.by_instance.get_mut(&(0, None)).unwrap()[0] ^= 1,
        }
        let internally_consistent_id = wrong.id();
        assert!(check_flat_candidate_record_v9(&wrong, &internally_consistent_id).is_err(), "field {field}");
    }
    let (p, pc) = fixture();
    for max in [0, misaka_palw_tir::program::HISTORY_BOUND_V1_HELD + 1] {
        assert!(prepare_flat_candidate_v9([1; 64], k2_tir_v4_descriptor().digest(), &p.encode(), &pc, max).is_err());
    }
    assert!(prepare_flat_candidate_v9([1; 64], k2_tir_v4_descriptor().digest(), &p.encode(), &Default::default(), 1).is_err());
    let mut wrong = p.encode();
    wrong.push(0);
    assert!(prepare_flat_candidate_v9([1; 64], k2_tir_v4_descriptor().digest(), &wrong, &pc, 1).is_err());
}
