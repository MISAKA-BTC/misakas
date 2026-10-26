use super::*;
use crate::palw_checkpoint_court_v1::tests::{HeldFixture, held_fixture};
use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
use crate::palw_step_leg::{PalwStepOpeningV1, step_merkle_path_v1};
const FORM: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 = crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;
const LADDER: u64 = 1 << 26;
fn answer(fx: &HeldFixture, checkpoint: u32, index: u32) -> PalwHeldDisclosureV1 {
    PalwHeldDisclosureV1::StateChunk {
        anchor: PalwAttnCheckpointAnchorV1 {
            leaf: fx.checkpoint_leaves[checkpoint as usize].clone(),
            opening: PalwStepOpeningV1 {
                leaf_index: u64::from(checkpoint),
                leaf_hash: fx.checkpoint_hashes[checkpoint as usize],
                siblings: step_merkle_path_v1(&fx.checkpoint_hashes, checkpoint as usize).unwrap(),
            },
        },
        chunk: PalwAttnChunkOpeningV1 {
            chunk_index: index,
            chunk_bytes: fx.chunks[checkpoint as usize][index as usize].clone(),
            siblings: crate::palw_state_chunk_map::palw_state_chunk_path_for_map_v1(
                &fx.binding.shape_profile,
                fx.checkpoint_leaves[checkpoint as usize].covered_decode_call,
                &fx.chunks[checkpoint as usize],
                index,
            )
            .unwrap(),
        },
    }
}
#[test]
fn held_chunk_auth_workspace_preserves_raw_identity_and_semantic_order() {
    let fx = held_fixture(true, 20, None);
    for checkpoint in [0, 9, 15] {
        let missing = PalwHeldMissingV1::StateChunk { checkpoint, chunk: 0 };
        let good = answer(&fx, checkpoint, 0);
        let raw = |a: &PalwHeldDisclosureV1| {
            palw_held_da_check_disclosure_v1(&fx.binding.committed_execution_root, &missing, &fx.binding, a, LADDER, FORM)
        };
        let native = |a: &PalwHeldDisclosureV1| {
            palw_held_da_state_chunk_auth_request_v2(&fx.binding.committed_execution_root, &missing, &fx.binding, a, LADDER, FORM)
                .unwrap()
                .check_v2()
        };
        assert_eq!(raw(&good), Ok(()));
        assert!(native(&good).is_ok());
        let bytes = borsh::to_vec(&good).unwrap();
        let mut invalid = good.clone();
        let PalwHeldDisclosureV1::StateChunk { chunk, .. } = &mut invalid else { unreachable!() };
        chunk.chunk_bytes[0] ^= 1;
        let Err(NativeDecodeErrorV1::Decode(error)) = native(&invalid) else { panic!("invalid membership stays semantic") };
        assert_eq!(raw(&invalid), Err(error));
        let mut invalid = good.clone();
        let PalwHeldDisclosureV1::StateChunk { anchor, .. } = &mut invalid else { unreachable!() };
        anchor.opening.siblings.clear();
        let Err(NativeDecodeErrorV1::Decode(error)) = native(&invalid) else { panic!("invalid anchor stays semantic") };
        assert_eq!(raw(&invalid), Err(error));
        assert_eq!(borsh::to_vec(&good).unwrap(), bytes);
    }
    // Native dispatch must not allocate a formatted hybrid name even when it
    // is only being queried by the request. Exact domain/bytes remain shared.
    use crate::palw_state_chunk_map as maps;
    for (actual, name) in [
        (maps::hybrid_state_chunk_map_id_v1(), maps::palw_hybrid_state_chunk_map_name_v1()),
        (maps::hybrid_state_chunk_map_id_v2(), maps::palw_hybrid_state_chunk_map_name_v2()),
        (maps::hybrid_state_chunk_map_id_v3(), maps::palw_hybrid_state_chunk_map_name_v3()),
        (maps::hybrid_state_chunk_map_id_v4(), maps::palw_hybrid_state_chunk_map_name_v4()),
        (maps::hybrid_state_chunk_map_id_v5(), maps::palw_hybrid_state_chunk_map_name_v5()),
    ] {
        assert_eq!(actual, crate::palw_step::state_chunk_map_id_v1(&name));
    }
    // Writer admission follows actual canonical input length across its
    // initial/growth boundary; no native-object/encoded-size ratio is used.
    for n in [0, 180, 181, 256, 257, 1025, 4097] {
        let span = 64 + 3 * 4 + n;
        let q = writer(256, span).unwrap();
        let mut actual = Vec::with_capacity(256);
        let mut peak = actual.capacity() as u64;
        for bytes in [&[0u8; 64][..], &[0u8; 4], &[0u8; 4], &(n as u32).to_le_bytes()[..], &vec![1u8; n as usize][..]] {
            let old = actual.capacity();
            actual.extend_from_slice(bytes);
            peak = peak.max((actual.capacity() + if actual.capacity() != old { old } else { 0 }) as u64);
        }
        assert_eq!(actual.len() as u64, span);
        assert!(q >= peak);
    }
}
#[test]
fn held_chunk_auth_workspace_quota_is_before_raw_validator_and_releases_scalar_scratch() {
    const CHILD: &str = "MISAKA_HELD_CHUNK_AUTH_CORE_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::held_chunk_auth_workspace_quota_is_before_raw_validator_and_releases_scalar_scratch", module_path!());
        let name = name.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let fx = held_fixture(true, 20, None);
    let missing = PalwHeldMissingV1::StateChunk { checkpoint: 9, chunk: 0 };
    let answer = answer(&fx, 9, 0);
    let request = || {
        palw_held_da_state_chunk_auth_request_v2(&fx.binding.committed_execution_root, &missing, &fx.binding, &answer, LADDER, FORM)
            .unwrap()
    };
    let q = request().required_bytes_v2();
    let ceiling = q.checked_mul(4).unwrap();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    for available in [0, q - 1, q] {
        let blocker = reserve_native_requests_v1(ceiling - available).unwrap();
        let outcome = request().check_v2();
        if available < q {
            assert!(matches!(outcome, Err(NativeDecodeErrorV1::Resource(_))));
        } else {
            outcome.unwrap();
        }
        let probe = reserve_native_requests_v1(available).unwrap();
        drop(probe);
        drop(blocker);
    }
    assert!(reserve_native_requests_v1(ceiling).is_ok());
    let nonchunk = PalwHeldMissingV1::StepRange { first: 0, count: 1 };
    assert!(matches!(
        palw_held_da_state_chunk_auth_request_v2(&fx.binding.committed_execution_root, &nonchunk, &fx.binding, &answer, LADDER, FORM),
        Err(NativeDecodeErrorV1::Resource(_))
    ));
}
