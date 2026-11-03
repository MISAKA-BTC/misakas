use kaspa_consensus_core::{
    Hash64,
    palw_bond_budget_v1::{PalwBondBudgetMirrorV1, PalwBondBudgetPolicyV1},
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1},
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error},
};

#[test]
fn active_round_fee_budget_defers_when_local_capacity_is_unavailable() {
    // This integration test owns its process-global resource admission.
    let limit = 16 * 1024 * 1024;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: limit }));
    let state = PalwChainStateV2::genesis();
    let params =
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0).unwrap().with_bond_budget(
            Some(PalwBondBudgetMirrorV1 { from_daa: 101, policy: PalwBondBudgetPolicyV1::unapproved_probe_v1(), allocation: None }),
        );
    let before = PalwBlockContextV2 { block: Hash64::from_u64_word(2), daa_score: 100, blue_score: 100, subsidy: 0 };
    let active = PalwBlockContextV2 { daa_score: 101, blue_score: 101, ..before };
    let lease = reserve_native_requests_v1(limit).unwrap().expect("quota is armed");
    assert!(state.round_fee_admission_v1(&params, &before, None).unwrap().is_none());
    assert!(matches!(state.round_fee_admission_v1(&params, &active, None), Err(PalwStateV2Error::NativeResource(_))));
    drop(lease);
    let budget = state.round_fee_admission_v1(&params, &active, None).unwrap().expect("active budget is constructed on retry");
    assert_eq!(budget.daa_score, active.daa_score);
    assert!(state.bond_budget().is_none(), "preflight leaves the accepted parent intact");
}
