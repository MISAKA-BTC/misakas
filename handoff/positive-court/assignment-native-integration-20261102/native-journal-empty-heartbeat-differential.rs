use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwStateParamsV2, apply_delta_v2, apply_palw_transition_retained_v2, revert_delta_v2},
};
fn main() {
    let params = PalwStateParamsV2::new(1000, 16, 16, 16, 16, 100, Hash64::from_bytes([4; 64]), 2, 1000, 1, 1, 1).unwrap();
    let points: Vec<_> = (1..=8).map(|n| PalwBlockContextV2 {
        block: Hash64::from_bytes([n;64]), daa_score: n.into(), blue_score: n.into(), subsidy: 0,
    }).collect();
    let mut untracked = PalwChainStateV2::genesis();
    let mut expected = Vec::new();
    for point in &points {
        let (state, delta) = apply_palw_transition_retained_v2(&untracked, &params, point, &[], None).unwrap();
        expected.push((state.state_root(), (*delta).clone()));
        untracked = (*state).clone();
    }
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 1 << 20, database_bytes: 0 }));
    let mut tracked = PalwChainStateV2::genesis();
    for (point, (expected_root, expected_delta)) in points.iter().zip(expected) {
        let (state, delta) = apply_palw_transition_retained_v2(&tracked, &params, point, &[], None).unwrap();
        assert_eq!(state.state_root(), expected_root);
        assert_eq!(*delta, expected_delta);
        assert_eq!(apply_delta_v2(&tracked, &delta, &params).unwrap().state_root(), expected_root);
        assert_eq!(revert_delta_v2(&state, &delta, &params).unwrap().state_root(), tracked.state_root());
        tracked = (*state).clone();
    }
    println!("8 metered empty transitions matched unmetered roots and deltas and survived forward/reverse replay");
}
