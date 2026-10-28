//! Actual tiny Q36 capture→retained accuser→GDN court controls. No campaign claim.
#![allow(warnings)]
mod common;
use common::*;
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::PalwAttemptRulesV1,
    palw_attempt_v2::palw_attempt_job_v1,
    palw_backend::PalwExecutionBackendV1,
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
    palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v2, qwen36_profile_v8},
    palw_step::{canonical_step_coordinates, kernel_semantics_id_v1},
    palw_step_refute::{
        KDESC_Q36_GDN_STEP, PalwStepRefuteError,
        memory_v2::{PalwGdnCourtErrorV2, check_gdn_step_refutation_v2},
    },
};
use misaka_palw_base0::{
    gdn_evidence_v2::{Base0GdnEvidenceV2, base0_gdn_step_evidence_v2},
    qwen36_backend::gdn_capture_owner_v3::binding_from_capture_v3,
};

#[test]
fn dense_and_folded_actual_captures_retain_owners_and_bind_native_identity_before_court() {
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    for (folded, profile) in [(false, qwen36_profile_v2(geometry).unwrap()), (true, qwen36_profile_v8(geometry).unwrap())] {
        let backend = qwen36_backend(&artifact, &profile, qwen36_held_canonical_v1(32)).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(31)).unwrap();
        let out = backend.execute(&palw_attempt_job_v1(job, true), &prompt).unwrap();
        assert_eq!(
            matches!(
                misaka_palw_base0::produce::base0_material_decode_any_v1(&out.material).unwrap(),
                misaka_palw_base0::produce::Base0RetentionV1::Folded(_)
            ),
            folded,
            "assert actual retained codec, not the profile label"
        );
        let prompt: Vec<u32> = prompt.into_iter().map(|n| n as u32).collect();
        let binding = binding_from_capture_v3(&out.material).unwrap();
        let owned = backend
            .gdn_accuser_view_retained_v3(&out.material, Some(&prompt), out.execution_root, profile.shape_profile_id())
            .unwrap();
        assert_eq!(owned.binding, *binding);
        assert!(!owned.tiles.leaves.is_empty());
        assert!(
            backend.gdn_accuser_view_retained_v3(&out.material, Some(&prompt), Hash64::default(), profile.shape_profile_id()).is_err()
        );
        assert!(backend.gdn_accuser_view_retained_v3(&out.material, Some(&prompt), out.execution_root, Hash64::default()).is_err());
        let mut foreign_prompt = prompt.clone();
        foreign_prompt[0] ^= 1;
        assert!(
            backend
                .gdn_accuser_view_retained_v3(&out.material, Some(&foreign_prompt), out.execution_root, profile.shape_profile_id())
                .is_err()
        );
        // The held generation reaches the anchored recurrence court. Legacy
        // profiles keep their explicit map semantics and are not promoted.
        if profile == qwen36_profile_v8(geometry).unwrap() {
            let leaf = (0..binding.step_leaf_count)
                .find(|i| {
                    canonical_step_coordinates(&profile, &binding.job_context, *i)
                        .and_then(|c| profile.resolve_node_slot(c.node_slot))
                        .is_some_and(|(n, _)| n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_GDN_STEP))
                })
                .unwrap();
            let coord = canonical_step_coordinates(&profile, &binding.job_context, leaf).unwrap();
            let Base0GdnEvidenceV2::Step(evidence) =
                base0_gdn_step_evidence_v2(&owned.view(), coord, || backend.gdn_recompute_kernels_v2().unwrap()).unwrap()
            else {
                panic!("honest checkpoint")
            };
            let (openings, root) = backend.gdn_inventory_openings_v2().unwrap();
            let oracle = openings.into_oracle_v2(root).unwrap();
            assert!(matches!(
                check_gdn_step_refutation_v2(
                    &evidence,
                    out.execution_root,
                    profile.shape_profile_id(),
                    oracle.oracle(),
                    owned.max_leaves
                ),
                Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))
            ));
        }
        let capped = qwen36_backend(&artifact, &profile, qwen36_held_canonical_v1(32)).with_step_ladder_cap(1);
        assert!(
            matches!(
                capped.gdn_accuser_view_retained_v3(&out.material, Some(&prompt), out.execution_root, profile.shape_profile_id()),
                Err(NativeDecodeErrorV1::Resource(_))
            ),
            "the actual dense-materialization/ladder refusal is typed before replay"
        );
        let mut malformed = out.material.clone();
        malformed.truncate(malformed.len() / 2);
        assert!(binding_from_capture_v3(&malformed).is_err());
    }
}

#[test]
fn capacity_refusal_precedes_dense_decode_or_folded_replay() {
    if std::env::var("MISAKA_GDN_CAPTURE_CAPACITY_CHILD").as_deref() != Ok("1") {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "capacity_refusal_precedes_dense_decode_or_folded_replay", "--nocapture", "--test-threads=1"])
            .env("MISAKA_GDN_CAPTURE_CAPACITY_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let mut captures = Vec::new();
    for (folded, profile) in [(false, qwen36_profile_v2(geometry).unwrap()), (true, qwen36_profile_v8(geometry).unwrap())] {
        let backend = qwen36_backend(&artifact, &profile, qwen36_held_canonical_v1(32)).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(32)).unwrap();
        let out = backend.execute(&palw_attempt_job_v1(job, true), &prompt).unwrap();
        assert_eq!(
            matches!(
                misaka_palw_base0::produce::base0_material_decode_any_v1(&out.material).unwrap(),
                misaka_palw_base0::produce::Base0RetentionV1::Folded(_)
            ),
            folded
        );
        let prompt: Vec<u32> = prompt.into_iter().map(|n| n as u32).collect();
        captures.push((backend, profile, out, prompt));
    }
    if std::env::var("MISAKA_GDN_OWNER_LIFETIME").as_deref() == Ok("1") {
        let (backend, profile, out, prompt) = &captures[1];
        let ceiling = 256 << 20;
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
        let owned =
            backend.gdn_accuser_view_retained_v3(&out.material, Some(prompt), out.execution_root, profile.shape_profile_id()).unwrap();
        assert!(
            kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(ceiling).is_err(),
            "returned accuser owner must keep its own exact reservation after construction returns"
        );
        drop(owned);
        let available = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(ceiling).unwrap();
        drop(available);
        return;
    }
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    for (backend, profile, out, prompt) in &captures {
        assert!(matches!(binding_from_capture_v3(&out.material), Err(NativeDecodeErrorV1::Resource(_))));
        assert!(matches!(
            backend.gdn_accuser_view_retained_v3(&out.material, Some(prompt), out.execution_root, profile.shape_profile_id()),
            Err(NativeDecodeErrorV1::Resource(_))
        ));
    }
}

#[test]
fn returned_folded_accuser_keeps_capacity_until_its_owner_is_dropped() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "capacity_refusal_precedes_dense_decode_or_folded_replay", "--nocapture", "--test-threads=1"])
        .env("MISAKA_GDN_CAPTURE_CAPACITY_CHILD", "1")
        .env("MISAKA_GDN_OWNER_LIFETIME", "1")
        .status()
        .unwrap();
    assert!(status.success());
}
