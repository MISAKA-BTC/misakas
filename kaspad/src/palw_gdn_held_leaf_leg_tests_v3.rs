//! Actual retained leaf-leg callers; tiny producer fixtures, no campaign claim.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeDecodeErrorV1, NativeHeapV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
use misaka_palw_base0::fp_recompute::Base0FpRecomputeError;
use misaka_palw_base0::gdn_evidence_v2::{base0_gdn_leaf_leg_retained_v3, base0_gdn_leg_v2};

struct Observed<'a, K> {
    inner: K,
    calls: &'a std::cell::Cell<usize>,
    ceiling: u64,
}
impl<K: Base0FpRecomputeKernelsV1> Base0FpRecomputeKernelsV1 for Observed<'_, K> {
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError> {
        assert!(reserve_native_requests_v1(self.ceiling).is_err(), "leg owner must still be live during the actual prefix use");
        self.calls.set(self.calls.get() + 1);
        self.inner.forward_no_capture(token, position)
    }
    fn state_chunks(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
        self.inner.state_chunks(profile, positions)
    }
    fn state_chunks_snapshot_retained_v4(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        self.inner.state_chunks_snapshot_retained_v4(profile, positions)
    }
}

#[test]
fn retained_actual_leaf_leg_matches_native_root_with_bulk_network_and_refuses_corrupt_chain() {
    let w = world(0);
    let plain = base0_gdn_leg_v2(&w.forged.view()).unwrap();
    let owned = base0_gdn_leaf_leg_retained_v3(&w.forged.view()).unwrap();
    assert_eq!(owned.leaves, plain.leaves);
    assert_eq!(owned.leaf_hashes, plain.leaf_hashes);
    assert_eq!(owned.merkle_root, plain.merkle_root);
    assert!(owned.chunks.is_empty());
    // Constructor-only alternate context: rebuild the actual native chain for
    // a bulk network-id append larger than CanonicalWriter's 512-byte start.
    // This is not a registered/accepted claim or another model execution.
    let mut binding = w.forged.binding.clone();
    binding.job_context.network_id = vec![b'N'; 3000];
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.checkpoint_profile.profile_hash();
    let mut leaves = w.forged.checkpoint_leaves.clone();
    let mut prev = kaspa_consensus_core::palw_step_leg::checkpoint_genesis_prev_v2(&ctx_hash);
    for leaf in &mut leaves {
        leaf.prev_checkpoint_leaf_hash = prev;
        prev = checkpoint_leaf_hash_v2(&ctx_hash, &profile_hash, &binding.state_chunk_map_id, leaf);
    }
    let plain = misaka_palw_base0::legs::Base0CheckpointCaptureV1::from_leaves_v1(
        &binding.job_context,
        &binding.shape_profile,
        &binding.checkpoint_profile,
        &leaves,
    )
    .unwrap();
    binding.checkpoint_merkle_root = plain.merkle_root;
    let mut material = w.forged.view();
    material.binding = &binding;
    material.checkpoint_leaves = &leaves;
    let owned = base0_gdn_leaf_leg_retained_v3(&material).unwrap();
    assert_eq!(owned.leaf_hashes, plain.leaf_hashes);
    assert_eq!(owned.merkle_root, plain.merkle_root);
    drop(material);
    leaves[0].prev_checkpoint_leaf_hash = Hash64::from_u64_word(0xBAD);
    let mut material = w.forged.view();
    material.binding = &binding;
    material.checkpoint_leaves = &leaves;
    assert!(matches!(base0_gdn_leaf_leg_retained_v3(&material), Err(NativeDecodeErrorV1::Decode(_))));
    assert!(matches!(
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, &w), &material, || w.backend.gdn_recompute_kernels_v2().unwrap()),
        Err(PalwGdnHeldErrorV2::Operation(_))
    ));
}

#[test]
fn actual_retained_held_open_refuses_other_cadence_even_for_honest_material() {
    let w = world(0);
    let mut binding = w.honest.binding.clone();
    binding.shape_profile.state_chunk_map_id = kaspa_consensus_core::palw_state_chunk_map::hybrid_state_chunk_map_id_v2();
    let mut material = w.honest.view();
    material.binding = &binding;
    assert_eq!(
        kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(&binding.shape_profile),
        kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall
    );
    assert!(matches!(base0_gdn_leaf_leg_retained_v3(&material), Err(NativeDecodeErrorV1::Resource(_))));
    assert!(
        matches!(
            palw_gdn_held_open_retained_v3(claim_of(&w.honest, &w), &material, || w.backend.gdn_recompute_kernels_v2().unwrap()),
            Err(PalwGdnHeldErrorV2::Resource(_))
        ),
        "unsupported cadence is withheld, never silently an honest None"
    );
}

#[test]
fn actual_leaf_leg_refusal_precedes_prefix_and_accusation_and_retries_with_owner_live() {
    const CHILD: &str = "MISAKA_HELD_GDN_LEAF_LEG_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::actual_leaf_leg_refusal_precedes_prefix_and_accusation_and_retries_with_owner_live", module_path!());
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
    let w = world(0);
    let mut case =
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, &w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
            .unwrap()
            .unwrap();
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    case.demanded = Some((0, 110));
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let ceiling = 256 << 20;
    let calls = std::cell::Cell::new(0);
    let mut kernels = || Observed { inner: w.backend.gdn_recompute_kernels_v2().unwrap(), calls: &calls, ceiling };
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    assert!(matches!(
        palw_gdn_held_open_retained_v3(claim_of(&w.honest, &w), &w.honest.view(), &mut kernels),
        Err(PalwGdnHeldErrorV2::Resource(_))
    ));
    assert!(matches!(
        base0_gdn_forged_checkpoint_demands_retained_v3(&w.forged.view(), case.candidates[0], case.index, &mut kernels),
        Err(NativeDecodeErrorV1::Resource(_))
    ));
    assert_eq!(calls.get(), 0);
    drop(blocker);
    // Observe a real prefix: only the retained leg is newly admitted at this
    // point, before candidate/answer construction, and retires on return.
    let index = base0_gdn_first_forged_checkpoint_retained_v3(&w.forged.view(), kernels()).unwrap();
    assert_eq!(index, Some(case.index));
    assert!(calls.get() > 0);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
    // Permit precisely the independent complete-answer copy but no following
    // leaf array: this reaches actual refutation assembly's new leg boundary.
    let batch_bytes = (case.answers.len() * std::mem::size_of::<PalwHeldDisclosureV1>()) as u64
        + case
            .answers
            .iter()
            .map(|a| a.as_ref().unwrap().native_heap_v1("held GDN complete answer batch").bytes().unwrap())
            .sum::<u64>();
    let blocker = reserve_native_requests_v1(ceiling - batch_bytes).unwrap();
    let signs = std::cell::Cell::new(0);
    let signer = |m: &[u8], c: &[u8]| {
        signs.set(signs.get() + 1);
        sign(m, c)
    };
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2),
        network_domain: Hash64::from_u64_word(0xD0),
        openings: &w.openings,
        oracle: &w.oracle,
        sign: &signer,
    };
    assert!(matches!(
        palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), &mut kernels, &[], 10_000, 20, &accuser),
        Err(PalwGdnHeldErrorV2::Resource(_))
    ));
    assert_eq!(signs.get(), 0);
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    drop(blocker);
    let action = palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), &mut kernels, &[], 10_000, 20, &accuser).unwrap();
    assert!(matches!(action, PalwGdnHeldNextRetainedV2::Accuse { .. }));
    assert_eq!(signs.get(), 1);
    drop(action);
    assert!(reserve_native_requests_v1(ceiling).is_ok(), "temporary leg/batch/action owners retire without retiring this old case");
    let leg = base0_gdn_leaf_leg_retained_v3(&w.forged.view()).unwrap();
    assert!(reserve_native_requests_v1(ceiling).is_err());
    assert_eq!(leg.merkle_root, w.forged.binding.checkpoint_merkle_root);
    drop(leg);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
}
