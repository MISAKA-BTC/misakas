use super::*;
use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};
use crate::palw_funded_execution_v11::PalwFundedExecutionFenceV11;

#[test]
fn presets_are_absent_and_never_normalizes_the_entire_peer_identity() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
        assert!(p.palw_funded_execution_v11.is_none());
        assert!(!p.palw_funded_execution_active_at_v11(u64::MAX));
        p.validate_palw_funded_execution_v11().unwrap();
        let before = (p.consensus_params_id(), p.consensus_schedule_id(), p.consensus_identity_id());
        let mut never = p.clone();
        never.palw_funded_execution_v11 = Some(PalwFundedExecutionFenceV11::unapproved_probe_at(ForkActivation::never(), [1; 64]));
        // Even a different policy value disappears with the entire option.
        never.palw_funded_execution_v11.as_mut().unwrap().policies.execution.reward_cap += 1;
        never.validate_palw_funded_execution_v11().unwrap();
        assert_eq!(before.2, never.consensus_identity_id(), "never tuple disappears from the normalized peer identity");
        assert_ne!(before.0, never.consensus_params_id(), "raw params report an explicitly declared tuple");
        assert_ne!(before.1, never.consensus_schedule_id(), "raw schedule reports a declaration even at never()");
    }
}
#[test]
fn height_and_every_policy_namespace_are_committed_and_only_activation_is_visited() {
    let mut p = TESTNET_PARAMS;
    p.palw_funded_execution_v11 = Some(PalwFundedExecutionFenceV11::unapproved_probe_at(ForkActivation::new(1000), [2; 64]));
    let before = (p.consensus_params_id(), p.consensus_schedule_id(), p.consensus_identity_id());
    assert!(p.validate_palw_funded_execution_v11().is_err());
    p.palw_funded_execution_v11.as_ref().unwrap().policies.check([2; 64]).unwrap();
    assert!(p.palw_funded_execution_v11.as_ref().unwrap().policies.check([3; 64]).is_err());
    for field in 0..4 {
        let mut changed = p.clone();
        let fence = changed.palw_funded_execution_v11.as_mut().unwrap();
        match field {
            0 => fence.activation = ForkActivation::new(1001),
            1 => fence.policies.execution.reward_cap += 1,
            2 => fence.policies.quota.actor.0[0] += 1,
            _ => fence.policies.discovery.requests_per_session += 1,
        }
        assert_ne!(before.0, changed.consensus_params_id(), "field {field}");
        assert_ne!(before.1, changed.consensus_schedule_id(), "field {field}");
        assert_eq!(before.2, changed.consensus_identity_id(), "a future fork is normalized away for deploy-time peer compatibility");
        let mut live = p.clone();
        live.palw_funded_execution_v11.as_mut().unwrap().activation = ForkActivation::new(0);
        let mut changed_live = changed.clone();
        changed_live.palw_funded_execution_v11.as_mut().unwrap().activation = ForkActivation::new(0);
        if field != 0 {
            assert_ne!(live.consensus_identity_id(), changed_live.consensus_identity_id());
        }
    }
    let policy = p.palw_funded_execution_v11.as_ref().unwrap().policies.clone();
    p.for_each_fence(&mut |score| *score = 2000);
    let after = p.palw_funded_execution_v11.as_ref().unwrap();
    assert_eq!(after.activation, ForkActivation::new(2000));
    assert_eq!(after.policies, policy, "activation visitor cannot rewrite horizons, money, counts or 21-axis caps");
}

#[test]
fn stage_contract_is_independent_and_never_collapses_its_entire_policy() {
    use crate::palw_flat_stage_v14::PalwStageExecutionFenceV14;
    let mut p = TESTNET_PARAMS;
    let historical = (p.consensus_params_id(), p.consensus_schedule_id(), p.consensus_identity_id());
    p.palw_stage_execution_v14 = Some(PalwStageExecutionFenceV14::unapproved_probe_at(ForkActivation::never(), [4; 64]));
    p.palw_stage_execution_v14.as_mut().unwrap().policies.source_liability += 1;
    assert_eq!(historical.2, p.consensus_identity_id(), "never stage tuple normalizes away from peer identity");
    p.validate_palw_stage_execution_v14().unwrap();
    p.palw_stage_execution_v14.as_mut().unwrap().activation = ForkActivation::new(100);
    p.palw_stage_execution_v14.as_ref().unwrap().policies.check([4; 64]).unwrap();
    assert!(p.validate_palw_stage_execution_v14().is_err());
    assert!(p.palw_funded_execution_v11.is_none(), "stage policy cannot grant text11 rights");
    let before = (p.consensus_params_id(), p.consensus_schedule_id(), p.consensus_identity_id());
    for field in 0..4 {
        let mut changed = p.clone();
        let f = changed.palw_stage_execution_v14.as_mut().unwrap();
        match field {
            0 => f.policies.version += 1,
            1 => f.policies.source_reservation += 1,
            2 => f.policies.source_maturity += 1,
            _ => f.policies.execution.discovery.requests_per_session += 1,
        }
        assert_ne!(before.0, changed.consensus_params_id());
        assert_ne!(before.1, changed.consensus_schedule_id());
        assert_eq!(before.2, changed.consensus_identity_id(), "future stage policy remains compatible before its fence");
        let mut live = p.clone();
        live.palw_stage_execution_v14.as_mut().unwrap().activation = ForkActivation::new(0);
        changed.palw_stage_execution_v14.as_mut().unwrap().activation = ForkActivation::new(0);
        assert_ne!(live.consensus_identity_id(), changed.consensus_identity_id());
    }
    let policy = p.palw_stage_execution_v14.as_ref().unwrap().policies.clone();
    p.for_each_fence(&mut |score| *score = 200);
    assert_eq!(p.palw_stage_execution_v14.as_ref().unwrap().activation, ForkActivation::new(200));
    assert_eq!(p.palw_stage_execution_v14.as_ref().unwrap().policies, policy);
    assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == "palw_stage_execution_v14"));
}

// Implemented for the integrated verification phase; not run during code-first work.
#[test]
fn native_final_policy_is_separate_and_all_values_leave_with_its_fence() {
    use crate::palw_native_final_v27::PalwNativeFinalFenceV27;
    for mut p in [MAINNET_PARAMS, TESTNET_PARAMS] {
        assert!(p.palw_native_final_v27.is_none());
        let baseline = (p.consensus_params_id(), p.consensus_schedule_id(), p.consensus_identity_id());
        p.palw_native_final_v27 = Some(PalwNativeFinalFenceV27::unapproved_probe_at(ForkActivation::never(), [8; 64]));
        assert_eq!(baseline.2, p.consensus_identity_id());
        assert_ne!(baseline.0, p.consensus_params_id());
        assert_ne!(baseline.1, p.consensus_schedule_id());
        p.validate_palw_native_final_v27().unwrap();
        let fence = p.palw_native_final_v27.as_mut().unwrap();
        fence.activation = ForkActivation::always();
        assert!(p.validate_palw_native_final_v27().is_err());
        let active = p.consensus_identity_id();
        p.palw_native_final_v27.as_mut().unwrap().policy.finder_profit_margin_sompi += 1;
        assert_ne!(active, p.consensus_identity_id(), "active numerical Final conditions are network identity");
        let before_queue = p.consensus_identity_id();
        p.palw_native_final_v27.as_mut().unwrap().policy.queue_policy.max_owned_bytes += 1;
        assert_ne!(before_queue, p.consensus_identity_id(), "original simultaneous queue promise is network identity");
        let policy = p.palw_native_final_v27.as_ref().unwrap().policy.clone();
        p.for_each_fence(&mut |score| *score = 1000);
        assert_eq!(p.palw_native_final_v27.as_ref().unwrap().policy, policy);
        assert_eq!(p.palw_native_final_v27.as_ref().unwrap().activation, ForkActivation::new(1000));
        assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == "palw_native_final_v27"));
        assert!(p.palw_funded_execution_v11.is_none());
        assert!(p.palw_stage_execution_v14.is_none());
    }
}

#[test]
fn stage28_does_not_grant_old14_and_preserves_future_peer_compatibility() {
    use crate::palw_flat_stage_v28::PalwStageExecutionFenceV28;
    let mut p = TESTNET_PARAMS;
    let before = (p.consensus_params_id(), p.consensus_schedule_id(), p.consensus_identity_id());
    p.palw_stage_execution_v28 = Some(PalwStageExecutionFenceV28::unapproved_probe_at(ForkActivation::never(), [10; 64]));
    assert!(p.palw_stage_execution_v14.is_none());
    assert!(!p.palw_stage_execution_active_at_v28(0));
    assert_eq!(p.consensus_identity_id(), before.2);
    assert_ne!(p.consensus_params_id(), before.0);
    assert_ne!(p.consensus_schedule_id(), before.1);
    p.validate_palw_stage_execution_v28().unwrap();
    p.palw_stage_execution_v28.as_mut().unwrap().activation = ForkActivation::new(1000);
    assert!(p.validate_palw_stage_execution_v28().is_err());
    assert_eq!(p.consensus_identity_id(), before.2);
    p.palw_stage_execution_v28.as_mut().unwrap().activation = ForkActivation::always();
    let active = p.consensus_identity_id();
    p.palw_stage_execution_v28.as_mut().unwrap().policies.maximum_horizon += 1;
    assert_ne!(p.consensus_identity_id(), active);
    let policy = p.palw_stage_execution_v28.as_ref().unwrap().policies.clone();
    p.for_each_fence(&mut |daa| *daa = 1500);
    assert_eq!(p.palw_stage_execution_v28.as_ref().unwrap().policies, policy);
    assert_eq!(p.palw_stage_execution_v28.as_ref().unwrap().activation, ForkActivation::new(1500));
}
