//! Native metadata construction only: this is not an execution/campaign fixture.
use super::*;
use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
use crate::palw_qwen36_profile::{QWEN36_35B_A3B, qwen36_profile_v8};

#[test]
fn actual_held_metadata_matches_dispatch_and_unknown_map_never_falls_back() {
    let profile = qwen36_profile_v8(QWEN36_35B_A3B).unwrap();
    let positions = (1..=1024).find(|n| cadence::palw_checkpoint_leaf_carries_recurrence_v1(&profile, *n)).unwrap();
    let (offset, count, layout) = gdn_recurrence_section_v2(&profile, positions).unwrap();
    let retained = gdn_recurrence_section_retained_v3(&profile, positions).unwrap();
    assert_eq!((retained.offset, retained.count), (offset, count));
    let (actual, legacy) = (retained._layout.as_ref().unwrap(), layout.as_ref().unwrap());
    assert_eq!(actual.attn.attn_layers, legacy.attn.attn_layers);
    assert_eq!(actual.gdn_layers, legacy.gdn_layers);
    assert!(!actual.attn.attn_layers.is_empty() && !actual.gdn_layers.is_empty());
    let mut unknown = profile;
    unknown.state_chunk_map_id = Hash64::from_u64_word(0xFFFF);
    assert!(matches!(gdn_recurrence_section_retained_v3(&unknown, positions), Err(PalwGdnCourtErrorV2::Evidence(_))));
    assert!(matches!(layers_request(u64::MAX), Err(PalwGdnCourtErrorV2::Resource(_))));
}

#[test]
fn metadata_admission_itself_refuses_before_construction_and_retains_layout_arrays() {
    const CHILD: &str = "MISAKA_GDN_RECURRENCE_METADATA_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::metadata_admission_itself_refuses_before_construction_and_retains_layout_arrays", module_path!());
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
    let profile = qwen36_profile_v8(QWEN36_35B_A3B).unwrap();
    let positions = (1..=1024).find(|n| cadence::palw_checkpoint_leaf_carries_recurrence_v1(&profile, *n)).unwrap();
    let before = borsh::to_vec(&profile).unwrap();
    let ceiling = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    assert!(matches!(gdn_recurrence_section_retained_v3(&profile, positions), Err(PalwGdnCourtErrorV2::Resource(_))));
    assert_eq!(borsh::to_vec(&profile).unwrap(), before);
    drop(blocker);
    let retained = gdn_recurrence_section_retained_v3(&profile, positions).unwrap();
    assert!(!retained._layout.as_ref().unwrap().gdn_layers.is_empty());
    assert!(reserve_native_requests_v1(ceiling).is_err(), "actual layout Vecs keep the metadata ticket");
    drop(retained);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
}
