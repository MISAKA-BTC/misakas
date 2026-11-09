//! Future obligation policy identity and historical compatibility controls.
use super::*;
use crate::palw_mode_v2::{PalwConsensusMode, palw_ruleset_id_v2};

fn with_retention_candidate() -> Params {
    let mut params = palw_t12_shipped_params();
    params.palw_model_panel_one_retention_v1 = Some(ForkActivation::new(PALW_T12_MODEL_PANEL_ONE_RETENTION_FENCE_DAA_V1));
    params.sync_palw_g14_gates_v1();
    params
}

#[test]
fn model_panel_one_retention_none_and_never_preserve_the_shipped_old_identities() {
    let old = palw_t12_shipped_params();
    old.validate_palw_v2().unwrap();
    assert!(old.palw_model_panel_one_retention_v1_fence().is_none());
    assert_eq!(old.consensus_params_id().to_string(), "edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189");
    assert_eq!(old.consensus_schedule_id().to_string(), "4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04");
    let mut never = old.clone();
    never.palw_model_panel_one_retention_v1 = Some(ForkActivation::never());
    never.sync_palw_g14_gates_v1();
    never.validate_palw_v2().unwrap();
    assert!(never.palw_model_panel_one_retention_v1_fence().is_none());
    assert_eq!(never.consensus_identity_id(), old.consensus_identity_id());
    assert_eq!(never.consensus_params_id(), old.consensus_params_id());
    assert_eq!(never.consensus_schedule_id(), old.consensus_schedule_id());
}

#[test]
fn model_panel_one_retention_future_boundary_is_independently_visited_and_mirrored() {
    let old = palw_t12_shipped_params();
    let scheduled = with_retention_candidate();
    scheduled.validate_palw_v2().unwrap();
    let at = PALW_T12_MODEL_PANEL_ONE_RETENTION_FENCE_DAA_V1;
    assert_eq!(at, 20_000);
    assert_eq!(scheduled.palw_model_panel_one_v1_fence().unwrap().daa_score(), 18_640);
    let fence = scheduled.palw_model_panel_one_retention_v1_fence().unwrap();
    assert!(!fence.is_active(at - 1));
    assert!(fence.is_active(at));
    assert!(fence.is_active(at + 1));
    assert_eq!(scheduled.consensus_identity_id(), old.consensus_identity_id(), "a future rule preserves the live handshake");
    assert_ne!(scheduled.consensus_params_id(), old.consensus_params_id());
    assert_ne!(scheduled.consensus_schedule_id(), old.consensus_schedule_id());
    assert!(scheduled.palw_fences_v1().contains(&("palw_model_panel_one_retention_v1", Some(fence))));
    assert!(crate::fork_id_v1::fork_id_gate_fences_v1(&scheduled).contains(&at));
    let PalwConsensusMode::ConsensusV2(old_bundle) = &old.palw_consensus_mode else { unreachable!() };
    let PalwConsensusMode::ConsensusV2(new_bundle) = &scheduled.palw_consensus_mode else { unreachable!() };
    assert_eq!(new_bundle.state.model_panel_one_retention_v1_from_daa(), Some(at));
    assert_eq!(palw_ruleset_id_v2(old_bundle), palw_ruleset_id_v2(new_bundle), "top-level mirror is borsh-skipped");
    let mut remapped = scheduled.clone();
    remapped.for_each_fence(&mut |daa| {
        if *daa == at {
            *daa += 1;
        }
    });
    remapped.sync_palw_g14_gates_v1();
    assert_eq!(remapped.palw_model_panel_one_retention_v1_fence().unwrap().daa_score(), at + 1);
    assert_ne!(remapped.consensus_schedule_id(), scheduled.consensus_schedule_id());
    assert_eq!(remapped.consensus_identity_id(), scheduled.consensus_identity_id());
}

#[test]
fn model_panel_one_retention_rollout_keeps_live_peers_until_the_disputed_fence() {
    use crate::fork_id_v1::{ForkIdVerdict, evaluate_fork_id_v1, fork_id_v1};
    let old = palw_t12_shipped_params();
    let scheduled = with_retention_candidate();
    let at = PALW_T12_MODEL_PANEL_ONE_RETENTION_FENCE_DAA_V1;
    // The actual public G14 build already crossed 18,640. Merely comparing
    // normalized identity cannot establish that its next-fence gate stays open.
    for daa in [18_640, 18_930, at - 1] {
        let old_id = fork_id_v1(&old, daa);
        let new_id = fork_id_v1(&scheduled, daa);
        assert_eq!(old_id.fired, new_id.fired);
        assert_eq!(new_id.next, at);
        assert!(!evaluate_fork_id_v1(&scheduled, daa, &old_id.fired.as_bytes(), old_id.next).refuses());
        assert!(!evaluate_fork_id_v1(&old, daa, &new_id.fired.as_bytes(), new_id.next).refuses());
    }
    // Once the rule actually changes, keeping an un-upgraded peer would hide
    // a consensus split. Both sides must retain their existing refusal policy.
    for daa in [at, at + 1] {
        let old_id = fork_id_v1(&old, daa);
        let new_id = fork_id_v1(&scheduled, daa);
        assert!(matches!(
            evaluate_fork_id_v1(&scheduled, daa, &old_id.fired.as_bytes(), old_id.next),
            ForkIdVerdict::DisagreePastFence { fired_through, .. } if fired_through == at
        ));
        assert!(evaluate_fork_id_v1(&old, daa, &new_id.fired.as_bytes(), new_id.next).refuses());
    }
}

#[test]
fn model_panel_one_retention_requires_preceding_route_court_gate_c_and_an_exact_mirror() {
    let scheduled = with_retention_candidate();
    let at = PALW_T12_MODEL_PANEL_ONE_RETENTION_FENCE_DAA_V1;
    let mut missing_route = scheduled.clone();
    missing_route.palw_model_panel_one_v1 = None;
    missing_route.sync_palw_g14_gates_v1();
    assert!(missing_route.validate_palw_model_panel_one_retention_v1().is_err());
    let mut late_route = scheduled.clone();
    late_route.palw_model_panel_one_v1 = Some(ForkActivation::new(at + 1));
    late_route.sync_palw_g14_gates_v1();
    assert!(late_route.validate_palw_model_panel_one_retention_v1().is_err());
    let mut missing_court = scheduled.clone();
    missing_court.palw_court_capacity_v1 = None;
    missing_court.sync_palw_g14_gates_v1();
    assert!(missing_court.validate_palw_model_panel_one_retention_v1().is_err());
    let mut missing_gate_c = scheduled.clone();
    missing_gate_c.palw_independently_work_certified_v1 = None;
    missing_gate_c.sync_palw_g14_gates_v1();
    assert!(missing_gate_c.validate_palw_model_panel_one_retention_v1().is_err());
    for dormant in [None, Some(ForkActivation::never())] {
        let mut stale_mirror = scheduled.clone();
        stale_mirror.palw_model_panel_one_retention_v1 = dormant;
        // Deliberately do not sync: an armed mirror under a dormant public
        // identity is invalid, not an implicit hidden consensus policy.
        assert!(stale_mirror.validate_palw_model_panel_one_retention_v1().is_err());
        stale_mirror.palw_consensus_mode = PalwConsensusMode::Disabled;
        stale_mirror.validate_palw_model_panel_one_retention_v1().unwrap();
    }
    let mut wrong_mirror = scheduled;
    let PalwConsensusMode::ConsensusV2(bundle) = &mut wrong_mirror.palw_consensus_mode else { unreachable!() };
    bundle.state = bundle.state.clone().with_model_panel_one_retention_v1(None);
    assert!(wrong_mirror.validate_palw_model_panel_one_retention_v1().is_err());
}

#[test]
fn model_panel_one_retention_stays_absent_on_historical_and_other_network_params() {
    let historical = palw_t12_pre_model_panel_one_params_v1();
    assert_eq!(historical.consensus_params_id().to_string(), "3e6da238306cfdb7e71599d5ef8fb314f1b3ff33a50673af1321f3da9cfa5b94");
    for params in [
        historical,
        palw_t12_launch_params_v1(),
        palw_t12_release_v7_params(),
        mainnet_shipped_params(),
        palw_rc_shipped_params(),
        devnet_shipped_params(),
    ] {
        assert!(params.palw_model_panel_one_retention_v1_fence().is_none());
        params.validate_palw_v2().unwrap();
        if let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode {
            assert_eq!(bundle.state.model_panel_one_retention_v1_from_daa(), None);
        }
    }
}
